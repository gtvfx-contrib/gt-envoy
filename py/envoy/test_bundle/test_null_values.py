"""Tests for null value and unresolved reference handling in env files."""
import json
import logging
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent.parent.parent))

from envoy._environment import EnvironmentManager


def _write_env_file(tmp_dir: str, data: dict) -> Path:
    """Write a JSON env file to a temp directory and return its path."""
    path = Path(tmp_dir) / "env.json"
    with open(path, "w", encoding="utf-8") as fh:
        json.dump(data, fh)
    return path


def test_null_top_level_skips():
    """A top-level null value should not add the variable to the environment."""
    print("Testing top-level null value is skipped...")

    with tempfile.TemporaryDirectory() as tmp:
        env_file = _write_env_file(tmp, {"MY_VAR": None})
        manager = EnvironmentManager(inherit_env=False)
        result = manager.load_env_from_files(env_file, base_env={})

    assert "MY_VAR" not in result, (
        "A null value should not set MY_VAR in the environment"
    )

    print("  ✅ null top-level value is skipped")


def test_null_top_level_warns(caplog=None):
    """A top-level null value should emit a log.warning."""
    print("Testing top-level null value emits a warning...")

    with tempfile.TemporaryDirectory() as tmp:
        env_file = _write_env_file(tmp, {"MY_VAR": None})
        manager = EnvironmentManager(inherit_env=False)

        with _capture_warnings() as warnings:
            manager.load_env_from_files(env_file, base_env={})

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
        env_file = _write_env_file(tmp, {"MY_VAR": None})
        manager = EnvironmentManager(inherit_env=False)
        result = manager.load_env_from_files(
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
        env_file = _write_env_file(tmp, {"+=MY_VAR": None})
        manager = EnvironmentManager(inherit_env=False)
        result = manager.load_env_from_files(
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
        env_file = _write_env_file(tmp, {"^=MY_VAR": None})
        manager = EnvironmentManager(inherit_env=False)
        result = manager.load_env_from_files(
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
        env_file = _write_env_file(tmp, {"?=MY_VAR": None})
        manager = EnvironmentManager(inherit_env=False)
        result = manager.load_env_from_files(env_file, base_env={})

    assert "MY_VAR" not in result, (
        "?= with null must not set the variable"
    )

    print("  ✅ ?= null does not set the variable")


def test_null_in_list_item_skipped():
    """A null item inside a list value should be omitted from the joined path."""
    print("Testing null item inside list is skipped...")

    with tempfile.TemporaryDirectory() as tmp:
        env_file = _write_env_file(
            tmp,
            {"MY_PATH": ["good/path", None, "another/good/path"]},
        )
        manager = EnvironmentManager(inherit_env=False)
        result = manager.load_env_from_files(env_file, base_env={})

    value = result.get("MY_PATH", "")
    assert EnvironmentManager.normalize_path("good/path") in value, "Valid items must still be present"
    assert EnvironmentManager.normalize_path("another/good/path") in value, "Valid items must still be present"
    assert "None" not in value, "The null item must not appear as the string 'None'"

    print("  ✅ null list item is omitted")


def test_unresolved_ref_in_list_skipped():
    """A list item containing an unresolved ${VAR} reference is omitted."""
    print("Testing list item with unresolved reference is omitted...")

    with tempfile.TemporaryDirectory() as tmp:
        env_file = _write_env_file(
            tmp,
            {
                "^=PYTHONPATH": [
                    "${MISSING_VAR}/site-packages",
                    "good/path",
                ]
            },
        )
        manager = EnvironmentManager(inherit_env=False)
        result = manager.load_env_from_files(env_file, base_env={})

    value = result.get("PYTHONPATH", "")
    assert EnvironmentManager.normalize_path("good/path") in value, "Resolved items must be present"
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
        env_file = _write_env_file(
            tmp,
            {"MY_PATH": ["${MISSING_VAR}/path", "good/path"]},
        )
        manager = EnvironmentManager(inherit_env=False)

        with _capture_warnings() as warnings:
            manager.load_env_from_files(env_file, base_env={})

    assert any("MISSING_VAR" in msg for msg in warnings), (
        "The warning should name the undefined variable"
    )

    print("  ✅ unresolved list item emits warning")


def test_resolved_list_items_kept():
    """All resolved list items must be present in the final value."""
    print("Testing fully resolved list items are all kept...")

    with tempfile.TemporaryDirectory() as tmp:
        env_file = _write_env_file(
            tmp,
            {"MY_PATH": ["path/one", "path/two", "path/three"]},
        )
        manager = EnvironmentManager(inherit_env=False)
        result = manager.load_env_from_files(env_file, base_env={})

    value = result.get("MY_PATH", "")
    for raw in ("path/one", "path/two", "path/three"):
        expected = EnvironmentManager.normalize_path(raw)
        assert expected in value, f"Expected '{expected}' to be in the path"

    print("  ✅ all resolved list items are retained")


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

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

def run_all_tests():
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
    success = run_all_tests()
    sys.exit(0 if success else 1)
