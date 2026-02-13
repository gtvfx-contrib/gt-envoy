"""Example usage of the ApplicationWrapper module."""

import os
import time
from pathlib import Path
from gt.app.wrapper import (
    ApplicationWrapper,
    WrapperConfig,
    ExecutionResult,
    create_wrapper
)


def example_basic():
    """Basic usage example."""
    print("=" * 60)
    print("BASIC EXAMPLE")
    print("=" * 60)
    
    config = WrapperConfig(
        executable="python",
        args=["--version"],
        capture_output=True
    )
    
    wrapper = ApplicationWrapper(config)
    result = wrapper.run()
    
    print(f"Return code: {result.return_code}")
    print(f"Output: {result.stdout}")
    print(f"Execution time: {result.execution_time:.2f}s")
    print()


def example_with_environment():
    """Example with environment variables."""
    print("=" * 60)
    print("ENVIRONMENT VARIABLES EXAMPLE")
    print("=" * 60)
    
    def pre_run():
        print("Setting up environment...")
    
    def post_run(result: ExecutionResult):
        print(f"Cleanup complete. Exit code: {result.return_code}")
    
    config = WrapperConfig(
        executable="python",
        args=["-c", "import os; print(f'MY_VAR={os.environ.get(\"MY_VAR\")}')"   ],
        env={"MY_VAR": "custom_value"},
        pre_run=pre_run,
        post_run=post_run,
        capture_output=True,
        stream_output=False
    )
    
    wrapper = ApplicationWrapper(config)
    result = wrapper.run()
    
    print(f"Output: {result.stdout}")
    print()


def example_with_timeout():
    """Example with timeout."""
    print("=" * 60)
    print("TIMEOUT EXAMPLE")
    print("=" * 60)
    
    config = WrapperConfig(
        executable="python",
        args=["-c", "import time; time.sleep(10); print('Done')"],
        timeout=2.0,  # Will timeout after 2 seconds
        raise_on_error=False
    )
    
    wrapper = ApplicationWrapper(config)
    result = wrapper.run()
    
    print(f"Timed out: {result.timed_out}")
    print(f"Success: {result.success}")
    print()


def example_with_callbacks():
    """Example with various callbacks."""
    print("=" * 60)
    print("CALLBACKS EXAMPLE")
    print("=" * 60)
    
    def on_start(pid: int):
        print(f"[CALLBACK] Process started with PID: {pid}")
    
    def on_output(line: str):
        print(f"[OUT] {line}")
    
    def on_error(line: str):
        print(f"[ERR] {line}")
    
    config = WrapperConfig(
        executable="python",
        args=["-c", "print('Line 1'); print('Line 2'); import sys; print('Error!', file=sys.stderr)"],
        on_start=on_start,
        on_output=on_output,
        on_error=on_error,
        stream_output=False,  # We're handling output with callbacks
        capture_output=True
    )
    
    wrapper = ApplicationWrapper(config)
    result = wrapper.run()
    print()


def example_context_manager():
    """Example using context manager."""
    print("=" * 60)
    print("CONTEXT MANAGER EXAMPLE")
    print("=" * 60)
    
    config = WrapperConfig(
        executable="python",
        args=["--version"],
        capture_output=True
    )
    
    with ApplicationWrapper(config) as wrapper:
        result = wrapper.run()
        print(f"Output: {result.stdout}")
    
    print("Context manager ensures cleanup")
    print()


def example_convenience_function():
    """Example using the convenience create_wrapper function."""
    print("=" * 60)
    print("CONVENIENCE FUNCTION EXAMPLE")
    print("=" * 60)
    
    # Simple one-liner wrapper creation
    wrapper = create_wrapper(
        "python",
        "-c",
        "print('Hello from wrapper!')",
        capture_output=True,
        timeout=5.0
    )
    
    result = wrapper.run()
    print(f"Output: {result.stdout}")
    print()


def example_error_handling():
    """Example with error handling."""
    print("=" * 60)
    print("ERROR HANDLING EXAMPLE")
    print("=" * 60)
    
    def pre_run():
        print("Pre-run check...")
        # Simulate pre-run validation
        if not os.path.exists("some_required_file.txt"):
            print("Warning: Required file not found, but continuing...")
    
    def post_run(result: ExecutionResult):
        if result.success:
            print("Post-run: Execution successful")
        else:
            print(f"Post-run: Execution failed with code {result.return_code}")
    
    config = WrapperConfig(
        executable="python",
        args=["-c", "import sys; sys.exit(1)"],  # Will exit with error
        pre_run=pre_run,
        post_run=post_run,
        raise_on_error=False,  # Don't raise exception
        continue_on_pre_run_error=True
    )
    
    wrapper = ApplicationWrapper(config)
    result = wrapper.run()
    
    if not result.success:
        print(f"Process failed as expected with code: {result.return_code}")
    print()


def example_working_directory():
    """Example with custom working directory."""
    print("=" * 60)
    print("WORKING DIRECTORY EXAMPLE")
    print("=" * 60)
    
    temp_dir = Path.cwd() / "temp"
    temp_dir.mkdir(exist_ok=True)
    
    config = WrapperConfig(
        executable="python",
        args=["-c", "import os; print(f'CWD: {os.getcwd()}')"],
        cwd=temp_dir,
        capture_output=True,
        stream_output=False
    )
    
    wrapper = ApplicationWrapper(config)
    result = wrapper.run()
    
    print(f"Output: {result.stdout}")
    print()


def example_real_world_scenario():
    """Real-world scenario: Running a build process."""
    print("=" * 60)
    print("REAL-WORLD SCENARIO: Build Process")
    print("=" * 60)
    
    build_start_time = None
    
    def pre_build():
        nonlocal build_start_time
        build_start_time = time.time()
        print("🔨 Starting build process...")
        print("   - Cleaning old artifacts...")
        print("   - Validating environment...")
        print("   - Build environment ready!")
    
    def post_build(result: ExecutionResult):
        duration = time.time() - build_start_time
        if result.success:
            print(f"✅ Build completed successfully in {duration:.2f}s")
            print("   - Artifacts generated")
            print("   - Running post-build validation...")
        else:
            print(f"❌ Build failed after {duration:.2f}s")
            print(f"   - Exit code: {result.return_code}")
    
    def on_output(line: str):
        # Filter and format build output
        if "error" in line.lower():
            print(f"   ❌ {line}")
        elif "warning" in line.lower():
            print(f"   ⚠️  {line}")
        else:
            print(f"   ℹ️  {line}")
    
    config = WrapperConfig(
        executable="python",
        args=["-c", """
import time
print('Compiling sources...')
time.sleep(0.5)
print('Linking libraries...')
time.sleep(0.5)
print('Build complete!')
"""],
        env={"BUILD_TYPE": "release", "VERBOSE": "1"},
        pre_run=pre_build,
        post_run=post_build,
        on_output=on_output,
        timeout=30.0,
        stream_output=False,  # Using custom on_output callback
        raise_on_error=True
    )
    
    wrapper = ApplicationWrapper(config)
    try:
        result = wrapper.run()
        print(f"\n📊 Build Statistics:")
        print(f"   - PID: {result.pid}")
        print(f"   - Duration: {result.execution_time:.2f}s")
        print(f"   - Status: {'SUCCESS' if result.success else 'FAILED'}")
    except Exception as e:
        print(f"Build process failed: {e}")
    print()


if __name__ == "__main__":
    # Run all examples
    examples = [
        example_basic,
        example_with_environment,
        example_with_callbacks,
        example_context_manager,
        example_convenience_function,
        example_working_directory,
        example_error_handling,
        example_timeout,
        example_real_world_scenario
    ]
    
    for example in examples:
        try:
            example()
        except Exception as e:
            print(f"Example failed: {e}\n")
    
    print("All examples completed!")
