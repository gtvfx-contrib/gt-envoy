"""Tests for null value and unresolved reference handling in env files."""
import json
import logging
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent.parent.parent))

from envoy._environment import EnvironmentManager


def _writeEnvFile(tmp_dir: str, data: dict) -> Path:
    """Write a JSON env file to a temp directory and return its path."""
    path = Path(tmp_dir) / "env.json"
    with open(path, "w", encoding="utf-8") as fh:
        json.dump(data, fh)
    return path


def test_null_top_level_skips():
    """A top-level null value should not add the variable to the environment."""
    print("Testing top-level null value is skipped...")

    with tempfile.TemporaryDirectory() as tmp:
        env_file = _writeEnvFile(tmp, {"MY_VAR": None})
        manager = EnvironmentManager(inherit_env=False)
        result = manager.loadEnvFromFiles(env_file, base_env={})

    assert "MY_VAR" not in result, (
        "A null value should not set MY_VAR in the environment"
    )

    print("  ✅ null top-level value is skipped")


def test_null_top_level_warns(caplog=None):
    """A top-level null value should emit a log.warning."""
    print("Testing top-level null value emits a warning...")

    with tempfile.TemporaryDirectory() as tmp:
        env_file = _writeEnvFile(tmp, {"MY_VAR": None})
        manager = EnvironmentManager(inherit_env=False)

        with _capture_warnings() as warnings:
            manager.loadEnvFromFiles(env_file, base_env={})

    assert any("MY_VAR" in msg for msg in warnings), (
        "A warning should be logged naming the skipped variable"
    )
    assert any("null" in msg for msg in warnings), (
        "The warning should mention 'null'"
    )

    print("  ✅ null top-level value emits warning")


def test_null_preserves_existing():
    """A top-level null value must not remove a variable already in the environment."""
    print("Testing null value preserves existing variable...")

    with tempfile.TemporaryDirectory() as tmp:
        env_file = _writeEnvFile(tmp, {"MY_VAR": None})
        manager = EnvironmentManager(inherit_env=False)
        result = manager.loadEnvFromFiles(
            env_file, base_env={"MY_VAR": "original"}
        )

    assert result.get("MY_VAR") == "original", (
        "A null value entry should not remove a variable already in the environment"
    )

    print("  ✅ null preserves existing variable")


def test_null_with_append_operator():
    """+=VAR with null value should leave the existing value unchanged."""
    print("Testing += with null skips and preserves existing value...")

    with tempfile.TemporaryDirectory() as tmp:
        env_file = _writeEnvFile(tmp, {"+=MY_VAR": None})
        manager = EnvironmentManager(inherit_env=False)
        result = manager.loadEnvFromFiles(
            env_file, base_env={"MY_VAR": "original"}
        )

    assert result.get("MY_VAR") == "original", (
        "+= with null must not modify the existing variable"
    )

    print("  ✅ += null skips and preserves existing value")


def test_null_with_prepend_operator():
    """^=VAR with null value should leave the existing value unchanged."""
    print("Testing ^= with null skips and preserves existing value...")

    with tempfile.TemporaryDirectory() as tmp:
        env_file = _writeEnvFile(tmp, {"^=MY_VAR": None})
        manager = EnvironmentManager(inherit_env=False)
        result = manager.loadEnvFromFiles(
            env_file, base_env={"MY_VAR": "original"}
        )

    assert result.get("MY_VAR") == "original", (
        "^= with null must not modify the existing variable"
    )

    print("  ✅ ^= null skips and preserves existing value")


def test_null_with_default_operator():
    """?=VAR with null value should not set the variable."""
    print("Testing ?= with null does not set the variable...")

    with tempfile.TemporaryDirectory() as tmp:
        env_file = _writeEnvFile(tmp, {"?=MY_VAR": None})
        manager = EnvironmentManager(inherit_env=False)
        result = manager.loadEnvFromFiles(env_file, base_env={})

    assert "MY_VAR" not in result, (
        "?= with null must not set the variable"
    )

    print("  ✅ ?= null does not set the variable")


def test_null_in_list_item_skipped():
    """A null item inside a list value should be omitted from the joined path."""
    print("Testing null item inside list is skipped...")

    with tempfile.TemporaryDirectory() as tmp:
        env_file = _writeEnvFile(
            tmp,
            {"MY_PATH": ["good/path", None, "another/good/path"]},
        )
        manager = EnvironmentManager(inherit_env=False)
        result = manager.loadEnvFromFiles(env_file, base_env={})

    value = result.get("MY_PATH", "")
    assert EnvironmentManager.normalizePath("good/path") in value, "Valid items must still be present"
    assert EnvironmentManager.normalizePath("another/good/path") in value, "Valid items must still be present"
    assert "None" not in value, "The null item must not appear as the string 'None'"

    print("  ✅ null list item is omitted")


def test_unresolved_ref_in_list_skipped():
    """A list item containing an unresolved ${VAR} reference is omitted."""
    print("Testing list item with unresolved reference is omitted...")

    with tempfile.TemporaryDirectory() as tmp:
        env_file = _writeEnvFile(
            tmp,
            {
                "^=PYTHONPATH": [
                    "${MISSING_VAR}/site-packages",
                    "good/path",
                ]
            },
        )
        manager = EnvironmentManager(inherit_env=False)
        result = manager.loadEnvFromFiles(env_file, base_env={})

    value = result.get("PYTHONPATH", "")
    assert EnvironmentManager.normalizePath("good/path") in value, "Resolved items must be present"
    sep = ";" if __import__("os").name == "nt" else ":"
    parts = value.split(sep) if value else []
    assert not any("MISSING_VAR" in p for p in parts), (
        "No partially-expanded item should appear in the result"
    )

    print("  ✅ list item with unresolved reference is omitted")


def test_unresolved_ref_in_list_warns():
    """A list item with an unresolved ${VAR} reference should emit a warning."""
    print("Testing unresolved list item emits a warning...")

    with tempfile.TemporaryDirectory() as tmp:
        env_file = _writeEnvFile(
            tmp,
            {"MY_PATH": ["${MISSING_VAR}/path", "good/path"]},
        )
        manager = EnvironmentManager(inherit_env=False)

        with _capture_warnings() as warnings:
            manager.loadEnvFromFiles(env_file, base_env={})

    assert any("MISSING_VAR" in msg for msg in warnings), (
        "The warning should name the undefined variable"
    )

    print("  ✅ unresolved list item emits warning")


def test_resolved_list_items_kept():
    """All resolved list items must be present in the final value."""
    print("Testing fully resolved list items are all kept...")

    with tempfile.TemporaryDirectory() as tmp:
        env_file = _writeEnvFile(
            tmp,
            {"MY_PATH": ["path/one", "path/two", "path/three"]},
        )
        manager = EnvironmentManager(inherit_env=False)
        result = manager.loadEnvFromFiles(env_file, base_env={})

    value = result.get("MY_PATH", "")
    for raw in ("path/one", "path/two", "path/three"):
        expected = EnvironmentManager.normalizePath(raw)
        assert expected in value, f"Expected '{expected}' to be in the path"

    print("  ✅ all resolved list items are retained")


def test_optional_ref_in_list_defined_included():
    """${?VAR} in a list item where VAR is defined — item must be included."""
    print("Testing ${?VAR} list item is included when VAR is defined...")

    with tempfile.TemporaryDirectory() as tmp:
        env_file = _writeEnvFile(
            tmp,
            {
                "environment": {
                    "^=MYPATH": [
                        "${?SITE_PKGS}/Python311/site-packages",
                        "C:/always/included",
                    ]
                },
                "environment_allowlist": ["SITE_PKGS"],
            },
        )
        import os
        os.environ["SITE_PKGS"] = "R:/pkgs"
        try:
            manager = EnvironmentManager(inherit_env=False)
            result = manager.prepareEnvironment(env_files=[env_file])
        finally:
            del os.environ["SITE_PKGS"]

    value = result.get("MYPATH", "")
    assert EnvironmentManager.normalizePath("R:/pkgs/Python311/site-packages") in value, (
        f"Optional item should be included when VAR is defined, got: {value!r}"
    )
    assert EnvironmentManager.normalizePath("C:/always/included") in value

    print("  ✅ ${?VAR} list item included when VAR is defined")


def test_optional_ref_in_list_undefined_silently_dropped():
    """${?VAR} in a list item where VAR is not defined — item silently dropped, no warning."""
    print("Testing ${?VAR} list item is silently dropped when VAR is undefined...")

    with tempfile.TemporaryDirectory() as tmp:
        env_file = _writeEnvFile(
            tmp,
            {
                "environment": {
                    "^=MYPATH": [
                        "${?SITE_PKGS}/Python311/site-packages",
                        "C:/always/included",
                    ]
                },
                "environment_allowlist": ["SITE_PKGS"],
            },
        )
        import os
        os.environ.pop("SITE_PKGS", None)
        manager = EnvironmentManager(inherit_env=False)

        with _capture_warnings() as warnings:
            result = manager.prepareEnvironment(env_files=[env_file])

    value = result.get("MYPATH", "")
    assert EnvironmentManager.normalizePath("C:/always/included") in value, (
        "Non-optional items should still be present"
    )
    assert "SITE_PKGS" not in value, "The optional item should not appear in the result"
    assert not any("SITE_PKGS" in msg for msg in warnings), (
        f"No warning should be emitted for an undefined ${'{?SITE_PKGS}'} ref, got: {warnings}"
    )

    print("  ✅ ${?VAR} list item silently dropped when VAR is undefined")


def test_optional_ref_scalar_undefined_silently_skipped():
    """${?VAR} in a scalar value where VAR is undefined — entire entry silently skipped."""
    print("Testing ${?VAR} scalar entry is silently skipped when VAR is undefined...")

    with tempfile.TemporaryDirectory() as tmp:
        env_file = _writeEnvFile(
            tmp,
            {
                "environment": {
                    "MY_SITE": "${?SITE_PKGS}/path",
                    "ALWAYS_SET": "value",
                },
                "environment_allowlist": ["SITE_PKGS"],
            },
        )
        import os
        os.environ.pop("SITE_PKGS", None)
        manager = EnvironmentManager(inherit_env=False)

        with _capture_warnings() as warnings:
            result = manager.prepareEnvironment(env_files=[env_file])

    assert "MY_SITE" not in result, (
        "Entry with undefined optional ref should be absent from result"
    )
    assert result.get("ALWAYS_SET") == "value"
    assert not any("SITE_PKGS" in msg for msg in warnings), (
        "No warning should be emitted for an optional ref"
    )

    print("  ✅ ${?VAR} scalar entry silently skipped when VAR is undefined")


def test_optional_ref_undefined_takes_priority_over_required_unresolved():
    """If ${?OPT} is undefined, item is silently dropped even if ${REQ} is also undefined."""
    print("Testing optional undefined takes priority over required unresolved (no double-warn)...")

    with tempfile.TemporaryDirectory() as tmp:
        env_file = _writeEnvFile(
            tmp,
            {
                "^=MYPATH": [
                    "${?OPTIONAL_GATE}/${REQUIRED_MISSING}/sub",
                    "good/path",
                ]
            },
        )
        manager = EnvironmentManager(inherit_env=False)

        with _capture_warnings() as warnings:
            result = manager.loadEnvFromFiles(env_file, base_env={})

    value = result.get("MYPATH", "")
    assert EnvironmentManager.normalizePath("good/path") in value
    assert "OPTIONAL_GATE" not in value
    assert "REQUIRED_MISSING" not in value
    assert not any("OPTIONAL_GATE" in msg for msg in warnings), (
        "No warning for the optional gate variable"
    )
    assert not any("REQUIRED_MISSING" in msg for msg in warnings), (
        "No warning when optional gate already caused silent drop"
    )

    print("  ✅ optional undefined gate silently drops item (no double-warning)")


def test_required_ref_still_warns_without_optional():
    """${VAR} (no ?) where VAR is undefined still emits a warning (regression check)."""
    print("Testing required ${VAR} still warns when undefined (regression)...")

    with tempfile.TemporaryDirectory() as tmp:
        env_file = _writeEnvFile(
            tmp,
            {"MY_PATH": ["${MISSING_VAR}/path", "good/path"]},
        )
        manager = EnvironmentManager(inherit_env=False)

        with _capture_warnings() as warnings:
            manager.loadEnvFromFiles(env_file, base_env={})

    assert any("MISSING_VAR" in msg for msg in warnings), (
        "Required unresolved refs should still emit a warning"
    )

    print("  ✅ required ${VAR} still warns when undefined")




class _capture_warnings:
    """Context manager that captures log.warning messages from envoy._environment."""

    def __init__(self):
        self._handler = None
        self._records: list[str] = []

    def __enter__(self) -> list[str]:
        self._handler = _ListHandler(self._records)
        self._handler.setLevel(logging.WARNING)
        logging.getLogger("envoy._environment").addHandler(self._handler)
        return self._records

    def __exit__(self, *_):
        logging.getLogger("envoy._environment").removeHandler(self._handler)


class _ListHandler(logging.Handler):
    """Logging handler that appends formatted messages to a list."""

    def __init__(self, target: list[str]):
        super().__init__()
        self._target = target

    def emit(self, record: logging.LogRecord) -> None:
        self._target.append(self.format(record))


# ---------------------------------------------------------------------------
# Runner
# ---------------------------------------------------------------------------

def runAllTests():
    """Run all null-value and unresolved-reference tests."""
    tests = [
        test_null_top_level_skips,
        test_null_top_level_warns,
        test_null_preserves_existing,
        test_null_with_append_operator,
        test_null_with_prepend_operator,
        test_null_with_default_operator,
        test_null_in_list_item_skipped,
        test_unresolved_ref_in_list_skipped,
        test_unresolved_ref_in_list_warns,
        test_resolved_list_items_kept,
        test_optional_ref_in_list_defined_included,
        test_optional_ref_in_list_undefined_silently_dropped,
        test_optional_ref_scalar_undefined_silently_skipped,
        test_optional_ref_undefined_takes_priority_over_required_unresolved,
        test_required_ref_still_warns_without_optional,
    ]

    print("=" * 60)
    print("Running Null Value / Unresolved Reference Tests")
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
