"""Tests for the ?= (default) operator."""
import sys
import os
import json
import tempfile
from pathlib import Path

# Add the module to path for testing
sys.path.insert(0, str(Path(__file__).parent.parent.parent.parent.parent))

from envoy._environment import EnvironmentManager
from envoy import WrapperConfig, ApplicationWrapper


def _writeEnvFile(tmp_dir: str, data: dict) -> Path:
    """Write a JSON env file to a temp directory and return its path."""
    path = Path(tmp_dir) / "env.json"
    with open(path, "w", encoding="utf-8") as fh:
        json.dump(data, fh)
    return path


def test_default_operator_sets_when_absent():
    """?= should set the variable when it is not already in merged_env."""
    print("Testing ?= sets variable when absent...")

    with tempfile.TemporaryDirectory() as tmp:
        env_file = _writeEnvFile(tmp, {"?=MY_VAR": "default_value"})

        manager = EnvironmentManager(inherit_env=False)
        result = manager.loadEnvFromFiles(env_file, base_env={})

    assert result.get("MY_VAR") == "default_value", (
        "?= should set MY_VAR when it is not present in merged_env"
    )

    print("  ✅ ?= sets variable when absent")


def test_default_operator_skips_when_present():
    """?= should NOT overwrite a variable that already exists in merged_env."""
    print("Testing ?= skips variable when already present...")

    with tempfile.TemporaryDirectory() as tmp:
        env_file = _writeEnvFile(tmp, {"?=MY_VAR": "default_value"})

        manager = EnvironmentManager(inherit_env=False)
        result = manager.loadEnvFromFiles(
            env_file, base_env={"MY_VAR": "existing_value"}
        )

    assert result.get("MY_VAR") == "existing_value", (
        "?= must not overwrite MY_VAR when it is already present in merged_env"
    )

    print("  ✅ ?= skips variable when already present")


def test_default_operator_inherit_env_mode():
    """?= should not override a variable that bubbles in from the full system env
    (inherit-env mode)."""
    print("Testing ?= interaction with inherit-env mode...")

    os.environ["ENVOY_TEST_DEFAULT_OP"] = "system_value"
    try:
        with tempfile.TemporaryDirectory() as tmp:
            env_file = _writeEnvFile(
                tmp, {"?=ENVOY_TEST_DEFAULT_OP": "file_default"}
            )

            # inherit_env=True seeds merged_env with the full os.environ, so
            # ENVOY_TEST_DEFAULT_OP is already present before the file is processed.
            manager = EnvironmentManager(inherit_env=True)
            result = manager.loadEnvFromFiles(
                env_file, base_env=dict(os.environ)
            )

        assert result.get("ENVOY_TEST_DEFAULT_OP") == "system_value", (
            "In inherit-env mode ?= must not override a variable that came "
            "from os.environ"
        )
    finally:
        del os.environ["ENVOY_TEST_DEFAULT_OP"]

    print("  ✅ ?= does not override system env variable in inherit-env mode")


def test_default_operator_closed_mode_allowlisted():
    """?= should not override a variable that entered merged_env via the
    allowlist in closed mode."""
    print("Testing ?= interaction with closed mode allowlisted variable...")

    os.environ["ENVOY_TEST_DEFAULT_OP"] = "allowlisted_value"
    try:
        with tempfile.TemporaryDirectory() as tmp:
            env_file = _writeEnvFile(
                tmp, {"?=ENVOY_TEST_DEFAULT_OP": "file_default"}
            )

            # Closed mode with ENVOY_TEST_DEFAULT_OP in the allowlist:
            # prepareEnvironment seeds merged_env with that var before
            # processing files, so ?= must leave it alone.
            manager = EnvironmentManager(
                inherit_env=False,
                allowlist={"ENVOY_TEST_DEFAULT_OP"},
            )
            # Simulate what prepareEnvironment does: seed allowlisted vars first.
            base = {}
            for var in manager.allowlist:
                if var in os.environ:
                    base[var] = os.environ[var]

            result = manager.loadEnvFromFiles(env_file, base_env=base)

        assert result.get("ENVOY_TEST_DEFAULT_OP") == "allowlisted_value", (
            "In closed mode ?= must not override a variable that was seeded "
            "from the allowlist"
        )
    finally:
        del os.environ["ENVOY_TEST_DEFAULT_OP"]

    print("  ✅ ?= does not override allowlisted variable in closed mode")


def test_default_operator_closed_mode_not_allowlisted():
    """?= should set a variable that is NOT in the allowlist in closed mode
    (the system value is not in scope, so the default should be applied)."""
    print("Testing ?= sets variable not present in closed mode (not allowlisted)...")

    os.environ["ENVOY_TEST_DEFAULT_OP"] = "system_value"
    try:
        with tempfile.TemporaryDirectory() as tmp:
            env_file = _writeEnvFile(
                tmp, {"?=ENVOY_TEST_DEFAULT_OP": "file_default"}
            )

            # Closed mode with an EMPTY allowlist: the system variable is not
            # seeded into merged_env, so ?= should apply the file default.
            manager = EnvironmentManager(inherit_env=False, allowlist=set())
            result = manager.loadEnvFromFiles(env_file, base_env={})

        assert result.get("ENVOY_TEST_DEFAULT_OP") == "file_default", (
            "In closed mode without allowlisting, ?= must apply the file "
            "default because the system variable is not in scope"
        )
    finally:
        del os.environ["ENVOY_TEST_DEFAULT_OP"]

    print("  ✅ ?= applies file default when system variable is not in scope")


def test_default_operator_via_wrapper():
    """End-to-end: ?= sets variable via ApplicationWrapper when not in env."""
    print("Testing ?= end-to-end via ApplicationWrapper...")

    # Make sure the variable is absent from the child process.
    os.environ.pop("ENVOY_TEST_E2E_DEFAULT", None)

    with tempfile.TemporaryDirectory() as tmp:
        env_file = _writeEnvFile(tmp, {"?=ENVOY_TEST_E2E_DEFAULT": "e2e_default"})

        config = WrapperConfig(
            executable="python",
            args=[
                "-c",
                "import os; print(os.environ.get('ENVOY_TEST_E2E_DEFAULT', 'MISSING'))",
            ],
            envFiles=str(env_file),
            capture_output=True,
            stream_output=False,
            log_execution=False,
        )

        wrapper = ApplicationWrapper(config)
        result = wrapper.run()

    assert result.success, "Wrapper should execute successfully"
    assert "e2e_default" in result.stdout, (  # type: ignore[operator]
        "Child process should see the default value set by ?="
    )

    print("  ✅ ?= end-to-end test passed")


def runAllTests():
    """Run all tests."""
    tests = [
        test_default_operator_sets_when_absent,
        test_default_operator_skips_when_present,
        test_default_operator_inherit_env_mode,
        test_default_operator_closed_mode_allowlisted,
        test_default_operator_closed_mode_not_allowlisted,
        test_default_operator_via_wrapper,
    ]

    print("=" * 60)
    print("Running ?= (default) Operator Tests")
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
    success = runAllTests()
    sys.exit(0 if success else 1)
