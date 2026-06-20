"""Tests for the ``environment_allowlist`` feature.

Covers:
- Var listed in ``environment_allowlist`` and present in ``os.environ``
  is seeded into the merge dict (available for ${VAR} expansion and +=).
- Var listed in ``environment_allowlist`` but absent from ``os.environ``
  is silently skipped.
- Var already set by ``base_env`` (or an earlier env file) is not
  overwritten by the allowlist pre-pass (base_env wins).
- ``+=`` and ``^=`` operators interact correctly with allowlist-seeded vars.
- Cross-file: allowlist declared in one file makes the var visible to
  operators in the same or earlier files (pre-pass runs across all files).
- ``allowlist_out`` parameter collects all declared var names.
- ``prepare_environment`` appends declared var names to ``ENVOY_ALLOWLIST``
  in the returned subprocess env.
"""
import json
import os
import sys
import tempfile
from pathlib import Path

# Ensure the package root is importable regardless of how pytest is invoked.
sys.path.insert(0, str(Path(__file__).parent.parent.parent))

from envoy._environment import EnvironmentManager


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _write_env_file(tmp_dir: str | Path, filename: str, data: dict) -> Path:
    """Write a JSON env file and return its path."""
    path = Path(tmp_dir) / filename
    with open(path, "w", encoding="utf-8") as fh:
        json.dump(data, fh)
    return path


# ---------------------------------------------------------------------------
# Tests — load_env_from_files (unit)
# ---------------------------------------------------------------------------

def test_allowlist_seeds_var_from_os_environ():
    """Var in environment_allowlist and in os.environ is seeded into merged_env."""
    print("Testing environment_allowlist seeds var from os.environ...")

    with tempfile.TemporaryDirectory() as tmp:
        os.environ["_ENVOY_TEST_ALLW_SEED"] = "host_value"
        try:
            env_file = _write_env_file(tmp, "env.json", {
                "environment": {},
                "environment_allowlist": ["_ENVOY_TEST_ALLW_SEED"],
            })
            manager = EnvironmentManager(inherit_env=False)
            result = manager.load_env_from_files(env_file, base_env={})
        finally:
            del os.environ["_ENVOY_TEST_ALLW_SEED"]

    assert result.get("_ENVOY_TEST_ALLW_SEED") == "host_value", (
        "environment_allowlist var should be seeded from os.environ into merged_env"
    )
    print("  \u2705 allowlist seeds var from os.environ")


def test_allowlist_skips_var_absent_from_os_environ():
    """Var in environment_allowlist but NOT in os.environ is silently skipped."""
    print("Testing environment_allowlist skips absent var...")

    # Ensure the var is not set
    os.environ.pop("_ENVOY_TEST_ALLW_ABSENT", None)

    with tempfile.TemporaryDirectory() as tmp:
        env_file = _write_env_file(tmp, "env.json", {
            "environment": {},
            "environment_allowlist": ["_ENVOY_TEST_ALLW_ABSENT"],
        })
        manager = EnvironmentManager(inherit_env=False)
        result = manager.load_env_from_files(env_file, base_env={})

    assert "_ENVOY_TEST_ALLW_ABSENT" not in result, (
        "Var absent from os.environ should not appear in merged_env"
    )
    print("  \u2705 allowlist silently skips var absent from os.environ")


def test_allowlist_does_not_overwrite_base_env():
    """base_env value wins — environment_allowlist does not overwrite it."""
    print("Testing environment_allowlist does not overwrite base_env value...")

    with tempfile.TemporaryDirectory() as tmp:
        os.environ["_ENVOY_TEST_ALLW_BASE"] = "host_value"
        try:
            env_file = _write_env_file(tmp, "env.json", {
                "environment": {},
                "environment_allowlist": ["_ENVOY_TEST_ALLW_BASE"],
            })
            manager = EnvironmentManager(inherit_env=False)
            result = manager.load_env_from_files(
                env_file,
                base_env={"_ENVOY_TEST_ALLW_BASE": "base_env_value"},
            )
        finally:
            del os.environ["_ENVOY_TEST_ALLW_BASE"]

    assert result.get("_ENVOY_TEST_ALLW_BASE") == "base_env_value", (
        "base_env value must win over os.environ when the var is in environment_allowlist"
    )
    print("  \u2705 base_env wins over allowlist seeding")


def test_allowlist_var_available_for_append_operator():
    """Var seeded via environment_allowlist is visible to += in the same file."""
    print("Testing environment_allowlist var is available for += operator...")

    host_val = EnvironmentManager.normalize_path("/host/bin")
    bundle_val = EnvironmentManager.normalize_path("/bundle/bin")

    with tempfile.TemporaryDirectory() as tmp:
        os.environ["_ENVOY_TEST_ALLW_PATH"] = host_val
        try:
            env_file = _write_env_file(tmp, "env.json", {
                "environment": {
                    "+=_ENVOY_TEST_ALLW_PATH": "/bundle/bin",
                },
                "environment_allowlist": ["_ENVOY_TEST_ALLW_PATH"],
            })
            manager = EnvironmentManager(inherit_env=False)
            result = manager.load_env_from_files(env_file, base_env={})
        finally:
            del os.environ["_ENVOY_TEST_ALLW_PATH"]

    path_sep = ";" if os.name == "nt" else ":"
    expected = f"{host_val}{path_sep}{bundle_val}"
    assert result.get("_ENVOY_TEST_ALLW_PATH") == expected, (
        f"Expected '{expected}', got '{result.get('_ENVOY_TEST_ALLW_PATH')}'"
    )
    print("  \u2705 allowlist-seeded var is available to += in the same file")


def test_allowlist_cross_file_visible_to_prepend():
    """Allowlist declared in file B makes var visible to ^= in file A (pre-pass is global)."""
    print("Testing cross-file: allowlist in file B visible to ^= in file A...")

    path_sep = ";" if os.name == "nt" else ":"
    host_val = EnvironmentManager.normalize_path("/host/bin")
    bundle_val = EnvironmentManager.normalize_path("/bundle/bin")

    with tempfile.TemporaryDirectory() as tmp:
        os.environ["_ENVOY_TEST_ALLW_CROSS"] = host_val
        try:
            # File A: uses ^= (would be a no-op if var isn't seeded yet)
            file_a = _write_env_file(tmp, "file_a.json", {
                "environment": {
                    "^=_ENVOY_TEST_ALLW_CROSS": "/bundle/bin",
                },
            })
            # File B: declares the allowlist (processed in pre-pass before file A's entries)
            file_b = _write_env_file(tmp, "file_b.json", {
                "environment": {},
                "environment_allowlist": ["_ENVOY_TEST_ALLW_CROSS"],
            })
            manager = EnvironmentManager(inherit_env=False)
            result = manager.load_env_from_files([file_a, file_b], base_env={})
        finally:
            del os.environ["_ENVOY_TEST_ALLW_CROSS"]

    expected = f"{bundle_val}{path_sep}{host_val}"
    assert result.get("_ENVOY_TEST_ALLW_CROSS") == expected, (
        f"Expected '{expected}', got '{result.get('_ENVOY_TEST_ALLW_CROSS')}' — "
        "cross-file pre-pass should seed the host var before file A's ^= runs"
    )
    print("  \u2705 cross-file pre-pass makes allowlist var visible to ^= in earlier file")


def test_allowlist_out_collects_declared_names():
    """allowlist_out receives every var name declared in environment_allowlist."""
    print("Testing allowlist_out collects declared var names...")

    with tempfile.TemporaryDirectory() as tmp:
        env_file = _write_env_file(tmp, "env.json", {
            "environment": {},
            "environment_allowlist": ["VAR_A", "VAR_B", "VAR_C"],
        })
        manager = EnvironmentManager(inherit_env=False)
        collected: list[str] = []
        manager.load_env_from_files(env_file, base_env={}, allowlist_out=collected)

    assert set(collected) == {"VAR_A", "VAR_B", "VAR_C"}, (
        f"Expected {{VAR_A, VAR_B, VAR_C}}, got {set(collected)}"
    )
    print("  \u2705 allowlist_out receives all declared var names")


# ---------------------------------------------------------------------------
# Tests — prepare_environment (integration)
# ---------------------------------------------------------------------------

def test_prepare_environment_appends_to_envoy_allowlist():
    """prepare_environment appends environment_allowlist var names to ENVOY_ALLOWLIST."""
    print("Testing prepare_environment appends to ENVOY_ALLOWLIST...")

    with tempfile.TemporaryDirectory() as tmp:
        env_file = _write_env_file(tmp, "env.json", {
            "environment": {},
            "environment_allowlist": ["_ENVOY_TEST_ALLW_INSPECT"],
        })
        # Ensure ENVOY_ALLOWLIST is not set in the host env for this test
        original_allowlist = os.environ.pop("ENVOY_ALLOWLIST", None)
        try:
            manager = EnvironmentManager(inherit_env=False)
            result = manager.prepare_environment(env_files=[env_file])
        finally:
            if original_allowlist is not None:
                os.environ["ENVOY_ALLOWLIST"] = original_allowlist

    envoy_allowlist = result.get("ENVOY_ALLOWLIST", "")
    listed_vars = {v.strip() for v in envoy_allowlist.replace(",", ";").split(";") if v.strip()}
    assert "_ENVOY_TEST_ALLW_INSPECT" in listed_vars, (
        f"_ENVOY_TEST_ALLW_INSPECT should appear in ENVOY_ALLOWLIST, got: '{envoy_allowlist}'"
    )
    print("  \u2705 ENVOY_ALLOWLIST contains var declared in environment_allowlist")


def test_prepare_environment_merges_with_existing_envoy_allowlist():
    """environment_allowlist additions are merged with the existing ENVOY_ALLOWLIST value."""
    print("Testing ENVOY_ALLOWLIST merge with existing value...")

    with tempfile.TemporaryDirectory() as tmp:
        env_file = _write_env_file(tmp, "env.json", {
            "environment": {},
            "environment_allowlist": ["_ENVOY_TEST_ALLW_NEW"],
        })
        original_allowlist = os.environ.get("ENVOY_ALLOWLIST")
        os.environ["ENVOY_ALLOWLIST"] = "EXISTING_VAR"
        try:
            manager = EnvironmentManager(inherit_env=False, allowlist={"EXISTING_VAR"})
            result = manager.prepare_environment(env_files=[env_file])
        finally:
            if original_allowlist is not None:
                os.environ["ENVOY_ALLOWLIST"] = original_allowlist
            else:
                os.environ.pop("ENVOY_ALLOWLIST", None)

    envoy_allowlist = result.get("ENVOY_ALLOWLIST", "")
    listed_vars = {v.strip() for v in envoy_allowlist.replace(",", ";").split(";") if v.strip()}
    assert "EXISTING_VAR" in listed_vars, (
        "Pre-existing ENVOY_ALLOWLIST vars must be preserved"
    )
    assert "_ENVOY_TEST_ALLW_NEW" in listed_vars, (
        "_ENVOY_TEST_ALLW_NEW should be added to ENVOY_ALLOWLIST"
    )
    print("  \u2705 ENVOY_ALLOWLIST merges existing and new allowlist vars")


# ---------------------------------------------------------------------------
# Runner (also usable as a standalone script via verify_features.py pattern)
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    tests = [
        test_allowlist_seeds_var_from_os_environ,
        test_allowlist_skips_var_absent_from_os_environ,
        test_allowlist_does_not_overwrite_base_env,
        test_allowlist_var_available_for_append_operator,
        test_allowlist_cross_file_visible_to_prepend,
        test_allowlist_out_collects_declared_names,
        test_prepare_environment_appends_to_envoy_allowlist,
        test_prepare_environment_merges_with_existing_envoy_allowlist,
    ]

    passed = 0
    failed = 0
    for test in tests:
        try:
            test()
            passed += 1
        except Exception as exc:
            print(f"  \u274c FAILED: {exc}")
            failed += 1

    print(f"\n{'=' * 50}")
    print(f"Results: {passed} passed, {failed} failed")
    if failed:
        sys.exit(1)
