"""Tests for the ApplicationWrapper module."""

import sys
import os
import tempfile
from pathlib import Path

# Add the module to path for testing
sys.path.insert(0, str(Path(__file__).parent / "bin" / "py"))

from gt.app.wrapper import (
    ApplicationWrapper,
    WrapperConfig,
    ExecutionResult,
    create_wrapper,
    WrapperError,
    ExecutionError
)


def test_basic_execution():
    """Test basic command execution."""
    print("Testing basic execution...")
    
    config = WrapperConfig(
        executable="python",
        args=["--version"],
        capture_output=True,
        log_execution=False
    )
    
    wrapper = ApplicationWrapper(config)
    result = wrapper.run()
    
    assert result.return_code == 0, "Basic execution should succeed"
    assert result.success, "Result should be marked as success"
    assert result.stdout is not None, "Should capture output"
    assert "Python" in result.stdout, "Should contain Python version"
    assert result.pid is not None, "Should have PID"
    
    print(f"  ✅ Basic execution test passed: {result}")


def test_environment_variables():
    """Test environment variable passing."""
    print("Testing environment variables...")
    
    config = WrapperConfig(
        executable="python",
        args=["-c", "import os; print(os.environ.get('TEST_VAR', 'not_found'))"],
        env={"TEST_VAR": "hello_world"},
        capture_output=True,
        stream_output=False,
        log_execution=False
    )
    
    wrapper = ApplicationWrapper(config)
    result = wrapper.run()
    
    assert result.success, "Should execute successfully"
    assert "hello_world" in result.stdout, "Should have custom env var"
    
    print(f"  ✅ Environment test passed: {result.stdout.strip()}")


def test_pre_post_run():
    """Test pre and post run operations."""
    print("Testing pre/post run operations...")
    
    executed = {"pre": False, "post": False, "result": None}
    
    def pre_run():
        executed["pre"] = True
    
    def post_run(result: ExecutionResult):
        executed["post"] = True
        executed["result"] = result
    
    config = WrapperConfig(
        executable="python",
        args=["-c", "print('test')"],
        pre_run=pre_run,
        post_run=post_run,
        capture_output=True,
        stream_output=False,
        log_execution=False
    )
    
    wrapper = ApplicationWrapper(config)
    result = wrapper.run()
    
    assert executed["pre"], "Pre-run should execute"
    assert executed["post"], "Post-run should execute"
    assert executed["result"] is not None, "Post-run should receive result"
    assert executed["result"].success, "Result should be successful"
    
    print("  ✅ Pre/post run test passed")


def test_timeout():
    """Test timeout functionality."""
    print("Testing timeout...")
    
    config = WrapperConfig(
        executable="python",
        args=["-c", "import time; time.sleep(5)"],
        timeout=1.0,
        raise_on_error=False,
        log_execution=False
    )
    
    wrapper = ApplicationWrapper(config)
    result = wrapper.run()
    
    assert result.timed_out, "Should timeout"
    assert not result.success, "Should not be successful"
    assert result.execution_time < 2.0, "Should stop before 2 seconds"
    
    print(f"  ✅ Timeout test passed: timed_out={result.timed_out}, time={result.execution_time:.2f}s")


def test_error_handling():
    """Test error handling."""
    print("Testing error handling...")
    
    # Test with raise_on_error=False
    config = WrapperConfig(
        executable="python",
        args=["-c", "import sys; sys.exit(42)"],
        raise_on_error=False,
        log_execution=False
    )
    
    wrapper = ApplicationWrapper(config)
    result = wrapper.run()
    
    assert result.return_code == 42, "Should capture exit code"
    assert not result.success, "Should not be successful"
    
    # Test with raise_on_error=True
    config.raise_on_error = True
    wrapper = ApplicationWrapper(config)
    
    try:
        wrapper.run()
        assert False, "Should have raised ExecutionError"
    except ExecutionError as e:
        assert "42" in str(e), "Error should mention exit code"
    
    print("  ✅ Error handling test passed")


def test_callbacks():
    """Test event callbacks."""
    print("Testing callbacks...")
    
    events = {"start": None, "output": [], "error": []}
    
    def on_start(pid):
        events["start"] = pid
    
    def on_output(line):
        events["output"].append(line)
    
    def on_error(line):
        events["error"].append(line)
    
    config = WrapperConfig(
        executable="python",
        args=["-c", "print('line1'); print('line2'); import sys; print('err1', file=sys.stderr)"],
        on_start=on_start,
        on_output=on_output,
        on_error=on_error,
        capture_output=True,
        stream_output=False,
        log_execution=False
    )
    
    wrapper = ApplicationWrapper(config)
    result = wrapper.run()
    
    assert events["start"] is not None, "on_start should be called"
    assert events["start"] == result.pid, "PID should match"
    assert len(events["output"]) == 2, "Should capture 2 stdout lines"
    assert "line1" in events["output"][0], "Should capture stdout"
    assert len(events["error"]) == 1, "Should capture 1 stderr line"
    assert "err1" in events["error"][0], "Should capture stderr"
    
    print(f"  ✅ Callbacks test passed: {len(events['output'])} stdout, {len(events['error'])} stderr")


def test_convenience_function():
    """Test create_wrapper convenience function."""
    print("Testing convenience function...")
    
    wrapper = create_wrapper(
        "python",
        "-c",
        "print('hello')",
        capture_output=True,
        log_execution=False,
        stream_output=False
    )
    
    result = wrapper.run()
    
    assert result.success, "Should execute successfully"
    assert "hello" in result.stdout, "Should capture output"
    
    print("  ✅ Convenience function test passed")


def test_working_directory():
    """Test working directory."""
    print("Testing working directory...")
    
    with tempfile.TemporaryDirectory() as tmpdir:
        config = WrapperConfig(
            executable="python",
            args=["-c", "import os; print(os.getcwd())"],
            cwd=tmpdir,
            capture_output=True,
            stream_output=False,
            log_execution=False
        )
        
        wrapper = ApplicationWrapper(config)
        result = wrapper.run()
        
        assert result.success, "Should execute successfully"
        # Normalize paths for comparison
        actual_cwd = os.path.normpath(result.stdout.strip())
        expected_cwd = os.path.normpath(tmpdir)
        assert actual_cwd == expected_cwd, f"Working directory should match: {actual_cwd} != {expected_cwd}"
    
    print("  ✅ Working directory test passed")


def run_all_tests():
    """Run all tests."""
    tests = [
        test_basic_execution,
        test_environment_variables,
        test_pre_post_run,
        test_timeout,
        test_error_handling,
        test_callbacks,
        test_convenience_function,
        test_working_directory
    ]
    
    print("=" * 60)
    print("Running ApplicationWrapper Tests")
    print("=" * 60)
    print()
    
    passed = 0
    failed = 0
    
    for test in tests:
        try:
            test()
            passed += 1
        except AssertionError as e:
            print(f"  ❌ {test.__name__} FAILED: {e}")
            failed += 1
        except Exception as e:
            print(f"  ❌ {test.__name__} ERROR: {e}")
            failed += 1
        print()
    
    print("=" * 60)
    print(f"Tests: {passed} passed, {failed} failed")
    print("=" * 60)
    
    return failed == 0


if __name__ == "__main__":
    success = run_all_tests()
    sys.exit(0 if success else 1)
