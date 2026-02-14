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
        result = wrapper.run() # type: ignore
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
    
    temp_dir = Path(__file__).parent.parent / "test_package" / "temp"
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


def example_env_from_files():
    """Example loading environment variables from JSON files."""
    print("=" * 60)
    print("ENVIRONMENT FROM FILES EXAMPLE")
    print("=" * 60)
    
    import json
    
    # Create temporary environment files
    temp_dir = Path(__file__).parent.parent / "test_package" / "temp"
    temp_dir.mkdir(exist_ok=True)
    
    # Base environment
    base_env = {
        "APP_NAME": "MyApp",
        "APP_VERSION": "1.0.0",
        "DEBUG": "false"
    }
    base_env_file = temp_dir / "base_env.json"
    with open(base_env_file, 'w') as f:
        json.dump(base_env, f, indent=2)
    
    # Override environment
    override_env = {
        "DEBUG": "true",
        "LOG_LEVEL": "verbose"
    }
    override_env_file = temp_dir / "override_env.json"
    with open(override_env_file, 'w') as f:
        json.dump(override_env, f, indent=2)
    
    # Use multiple JSON files (later files override earlier ones)
    config = WrapperConfig(
        executable="python",
        args=["-c", """
import os
print(f"APP_NAME: {os.environ.get('APP_NAME')}")
print(f"APP_VERSION: {os.environ.get('APP_VERSION')}")
print(f"DEBUG: {os.environ.get('DEBUG')}")
print(f"LOG_LEVEL: {os.environ.get('LOG_LEVEL')}")
print(f"CUSTOM: {os.environ.get('CUSTOM')}")
"""],
        env_files=[base_env_file, override_env_file],  # Multiple files merged
        env={"CUSTOM": "from_dict"},  # This overrides files
        capture_output=True,
        stream_output=False,
        inherit_env=False  # Only use our specified env
    )
    
    wrapper = ApplicationWrapper(config)
    result = wrapper.run()
    
    print("Environment priority: system < file1 < file2 < env dict")
    print(f"Output:\n{result.stdout}")
    
    # Cleanup
    base_env_file.unlink()
    override_env_file.unlink()
    print()


def example_list_paths():
    """Example using list-based paths with Unix format."""
    print("=" * 60)
    print("LIST-BASED PATHS EXAMPLE")
    print("=" * 60)
    
    import json
    
    # Create temporary environment file
    temp_dir = Path(__file__).parent.parent / "test_package" / "temp"
    temp_dir.mkdir(exist_ok=True)
    
    # Define paths as lists using Unix format (/)
    env_with_lists = {
        "PYTHONPATH": [
            "R:/repo/project1/py",
            "R:/repo/project2/py",
            "R:/repo/project3/py"
        ],
        "PATH": [
            "C:/Python312",
            "C:/Python312/Scripts",
            "R:/repo/tools/bin"
        ],
        "+=CUSTOM_PATH": [
            "C:/extra/path1",
            "C:/extra/path2"
        ],
        "SINGLE_VAR": "R:/some/single/path"
    }
    
    env_file = temp_dir / "list_paths.json"
    with open(env_file, 'w') as f:
        json.dump(env_with_lists, f, indent=2)
    
    # Set up existing environment for append demo
    os.environ["CUSTOM_PATH"] = "C:/original/custom"
    
    config = WrapperConfig(
        executable="python",
        args=["-c", """
import os
import sys
print("PYTHONPATH:")
pythonpath = os.environ.get('PYTHONPATH', '')
for p in pythonpath.split(';' if sys.platform == 'win32' else ':'):
    if p:
        print(f"  - {p}")

print()
print("PATH (first 3):")
path = os.environ.get('PATH', '')
for i, p in enumerate(path.split(';' if sys.platform == 'win32' else ':')):
    if i < 3 and p:
        print(f"  - {p}")

print()
print("CUSTOM_PATH:")
custom = os.environ.get('CUSTOM_PATH', '')
for p in custom.split(';' if sys.platform == 'win32' else ':'):
    if p:
        print(f"  - {p}")
"""],
        env_files=env_file,
        capture_output=True,
        stream_output=False,
        inherit_env=False
    )
    
    wrapper = ApplicationWrapper(config)
    result = wrapper.run()
    
    print("Benefits of list-based paths:")
    print("  ✓ Cleaner, more readable JSON")
    print("  ✓ Unix-style paths (/) work cross-platform")
    print("  ✓ Automatically joined with OS separator")
    print()
    print(f"Output:\n{result.stdout}")
    
    # Cleanup
    env_file.unlink()
    print()


def example_env_append_prepend():
    """Example demonstrating append/prepend to environment variables."""
    print("=" * 60)
    print("ENVIRONMENT APPEND/PREPEND EXAMPLE")
    print("=" * 60)
    
    import json
    
    # Create temporary environment files
    temp_dir = Path(__file__).parent.parent / "test_package" / "temp"
    temp_dir.mkdir(exist_ok=True)
    
    # Set a base PYTHONPATH for demonstration
    os.environ["MY_CUSTOM_PATH"] = "C:\\original\\path"
    
    # Method 1: Using {$VARNAME} syntax
    print("Method 1: Using {$VARNAME} syntax")
    append_env = {
        "MY_CUSTOM_PATH": "{$MY_CUSTOM_PATH};C:\\appended\\path",
        "NEW_VAR": "Initial value"
    }
    append_file = temp_dir / "append_env.json"
    with open(append_file, 'w') as f:
        json.dump(append_env, f, indent=2)
    
    # Prepend to variable (and chain multiple files)
    prepend_env = {
        "MY_CUSTOM_PATH": "C:\\prepended\\path;{$MY_CUSTOM_PATH}",
        "NEW_VAR": "{$NEW_VAR} + more"
    }
    prepend_file = temp_dir / "prepend_env.json"
    with open(prepend_file, 'w') as f:
        json.dump(prepend_env, f, indent=2)
    
    config = WrapperConfig(
        executable="python",
        args=["-c", """
import os
print(f"MY_CUSTOM_PATH: {os.environ.get('MY_CUSTOM_PATH')}")
print(f"NEW_VAR: {os.environ.get('NEW_VAR')}")
"""],
        env_files=[append_file, prepend_file],
        capture_output=True,
        stream_output=False
    )
    
    wrapper = ApplicationWrapper(config)
    result = wrapper.run()
    
    print(f"Output:\n{result.stdout}")
    
    # Method 2: Using += and ^= operators
    print("\nMethod 2: Using += and ^= operators")
    
    # Reset for demonstration
    os.environ["MY_CUSTOM_PATH"] = "C:\\original\\path"
    
    # Append operator
    append_op_env = {
        "+=MY_CUSTOM_PATH": "C:\\appended\\with\\operator"
    }
    append_op_file = temp_dir / "append_op_env.json"
    with open(append_op_file, 'w') as f:
        json.dump(append_op_env, f, indent=2)
    
    # Prepend operator
    prepend_op_env = {
        "^=MY_CUSTOM_PATH": "C:\\prepended\\with\\operator"
    }
    prepend_op_file = temp_dir / "prepend_op_env.json"
    with open(prepend_op_file, 'w') as f:
        json.dump(prepend_op_env, f, indent=2)
    
    config2 = WrapperConfig(
        executable="python",
        args=["-c", "import os; print(f'MY_CUSTOM_PATH: {os.environ.get(\"MY_CUSTOM_PATH\")}')"  ],
        env_files=[append_op_file, prepend_op_file],
        capture_output=True,
        stream_output=False
    )
    
    wrapper2 = ApplicationWrapper(config2)
    result2 = wrapper2.run()
    
    print(f"Output:\n{result2.stdout}")
    
    print("\nOperator Syntax Summary:")
    print("  +=VAR  : Appends value to existing variable")
    print("  ^=VAR  : Prepends value to existing variable")
    print("  VAR    : Replaces value (supports {$VAR} expansion)")
    
    # Cleanup
    append_file.unlink()
    prepend_file.unlink()
    append_op_file.unlink()
    prepend_op_file.unlink()
    print()

def example_special_variables():
    """Example using special wrapper variables like {$__PACKAGE__}."""
    print("=" * 60)
    print("SPECIAL WRAPPER VARIABLES EXAMPLE")
    print("=" * 60)
    
    import json
    
    # Create a package structure with env directory
    temp_dir = Path(__file__).parent.parent / "test_package" / "temp"
    temp_dir.mkdir(exist_ok=True)
    
    # Create package directory structure
    package_dir = temp_dir / "my_package"
    package_dir.mkdir(exist_ok=True)
    env_dir = package_dir / "env"
    env_dir.mkdir(exist_ok=True)
    
    # Create environment file using special variables
    env_config = {
        "+=PYTHONPATH": [
            "{$__PACKAGE__}/py",
            "{$__PACKAGE__}/lib/python"
        ],
        "+=PATH": "{$__PACKAGE__}/bin",
        "APP_ROOT": "{$__PACKAGE__}",
        "APP_NAME": "{$__PACKAGE_NAME__}",
        "CONFIG_FILE": "{$__PACKAGE__}/config/app.conf",
        "ENV_FILE_PATH": "{$__FILE__}"
    }
    
    env_file = env_dir / "config.json"
    with open(env_file, 'w') as f:
        json.dump(env_config, f, indent=2)
    
    config = WrapperConfig(
        executable="python",
        args=["-c", """
import os
import sys
print("Special wrapper variables resolved:")
print()
print(f"APP_ROOT: {os.environ.get('APP_ROOT')}")
print(f"APP_NAME: {os.environ.get('APP_NAME')}")
print(f"CONFIG_FILE: {os.environ.get('CONFIG_FILE')}")
print()
print("PYTHONPATH additions:")
pythonpath = os.environ.get('PYTHONPATH', '')
for p in pythonpath.split(';' if sys.platform == 'win32' else ':'):
    if 'my_package' in p and p:
        print(f"  - {p}")
print()
print(f"ENV_FILE_PATH: {os.environ.get('ENV_FILE_PATH')}")
"""],
        env_files=env_file,
        capture_output=True,
        stream_output=False,
        inherit_env=False
    )
    
    wrapper = ApplicationWrapper(config)
    result = wrapper.run()
    
    print("Special variables available:")
    print("  {$__PACKAGE__}      - Package root (parent of env/)")
    print("  {$__PACKAGE_ENV__}  - The env/ directory")
    print("  {$__PACKAGE_NAME__} - Package directory name")
    print("  {$__FILE__}         - Current env file path")
    print()
    print(f"Output:\n{result.stdout}")
    
    print("Benefits:")
    print("  ✓ Portable across different installations")
    print("  ✓ No hard-coded paths")
    print("  ✓ Works with version control")
    print()
    
    # Cleanup
    import shutil
    shutil.rmtree(package_dir)
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
        duration = time.time() - build_start_time # type: ignore
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
        example_env_from_files,
        example_list_paths,
        example_env_append_prepend,
        example_special_variables,
        example_error_handling,
        example_with_timeout,
        example_real_world_scenario
    ]
    
    for example in examples:
        try:
            example()
        except Exception as e:
            print(f"Example failed: {e}\n")
    
    print("All examples completed!")
