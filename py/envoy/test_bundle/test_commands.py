"""Tests for CommandRegistry.resolveEnvironment()."""

import sys
from pathlib import Path

# Add the module to path for testing
sys.path.insert(0, str(Path(__file__).parent.parent.parent.parent.parent))

from envoy._commands import CommandDefinition, CommandRegistry
from envoy._exceptions import WrapperError


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _makeRegistry(*commands: tuple) -> CommandRegistry:
    """Build a CommandRegistry from (name, environment_list) tuples.

    This bypasses the JSON-loading path so tests don't need real files on disk.
    """
    registry = CommandRegistry()
    for name, environment in commands:
        registry._commands[name] = CommandDefinition(
            name=name,
            environment=environment,
            envoy_env_dir=Path("/fake/dir"),
        )
    return registry


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------

def test_simple_resolution():
    """Plain file entries (containing a dot) are returned as-is."""
    print("Testing simple environment resolution...")

    registry = _makeRegistry(("myapp", ["base_env.json", "myapp_env.json"]))
    result = registry.resolveEnvironment("myapp")

    assert result == [
        ("base_env.json", Path("/fake/dir")),
        ("myapp_env.json", Path("/fake/dir")),
    ], f"Unexpected result: {result}"

    print("  ✅ Simple resolution test passed")


def test_recursive_resolution_order():
    """A command reference (no dot) is spliced in place, preserving order."""
    print("Testing recursive resolution order...")

    # "child" references "base" then appends its own file
    registry = _makeRegistry(
        ("base", ["base_env.json"]),
        ("child", ["base", "child_env.json"]),
    )
    result = registry.resolveEnvironment("child")

    assert result == [
        ("base_env.json", Path("/fake/dir")),
        ("child_env.json", Path("/fake/dir")),
    ], f"Unexpected result: {result}"

    print("  ✅ Recursive resolution order test passed")


def test_multi_level_recursion_order():
    """Multi-level references are resolved depth-first in declaration order."""
    print("Testing multi-level recursion order...")

    registry = _makeRegistry(
        ("base",  ["base_env.json"]),
        ("mid",   ["base", "mid_env.json"]),
        ("top",   ["mid", "top_env.json"]),
    )
    result = registry.resolveEnvironment("top")

    assert result == [
        ("base_env.json", Path("/fake/dir")),
        ("mid_env.json",  Path("/fake/dir")),
        ("top_env.json",  Path("/fake/dir")),
    ], f"Unexpected result: {result}"

    print("  ✅ Multi-level recursion order test passed")


def test_mixed_files_and_references():
    """File entries and command references can be freely mixed."""
    print("Testing mixed files and command references...")

    registry = _makeRegistry(
        ("base",  ["base_env.json"]),
        ("mixed", ["pre.json", "base", "post.json"]),
    )
    result = registry.resolveEnvironment("mixed")

    assert result == [
        ("pre.json",      Path("/fake/dir")),
        ("base_env.json", Path("/fake/dir")),
        ("post.json",     Path("/fake/dir")),
    ], f"Unexpected result: {result}"

    print("  ✅ Mixed files and references test passed")


def test_missing_referenced_command():
    """Referencing a command that does not exist raises WrapperError."""
    print("Testing missing referenced command...")

    registry = _makeRegistry(("cmd", ["nonexistent"]))

    try:
        registry.resolveEnvironment("cmd")
        assert False, "Should have raised WrapperError"
    except WrapperError as e:
        assert "nonexistent" in str(e), f"Error message should mention the missing command: {e}"

    print("  ✅ Missing referenced command test passed")


def test_direct_missing_command():
    """Resolving a command that does not exist at all raises WrapperError."""
    print("Testing direct missing command...")

    registry = _makeRegistry()  # empty registry

    try:
        registry.resolveEnvironment("ghost")
        assert False, "Should have raised WrapperError"
    except WrapperError as e:
        assert "ghost" in str(e), f"Error message should mention the missing command: {e}"

    print("  ✅ Direct missing command test passed")


def test_circular_reference():
    """A cycle in command references raises WrapperError."""
    print("Testing circular reference detection...")

    # a -> b -> a  (direct cycle)
    registry = _makeRegistry(
        ("a", ["b"]),
        ("b", ["a"]),
    )

    try:
        registry.resolveEnvironment("a")
        assert False, "Should have raised WrapperError"
    except WrapperError as e:
        assert "circular" in str(e).lower() or "cycle" in str(e).lower(), (
            f"Error message should mention circular reference: {e}"
        )

    print("  ✅ Circular reference test passed")


def test_self_referential_command():
    """A command that references itself raises WrapperError."""
    print("Testing self-referential command...")

    registry = _makeRegistry(("selfref", ["selfref"]))

    try:
        registry.resolveEnvironment("selfref")
        assert False, "Should have raised WrapperError"
    except WrapperError as e:
        assert "selfref" in str(e), f"Error message should mention the command: {e}"

    print("  ✅ Self-referential command test passed")


# ---------------------------------------------------------------------------
# Runner
# ---------------------------------------------------------------------------

def runAllTests():
    """Run all resolveEnvironment tests."""
    tests = [
        test_simple_resolution,
        test_recursive_resolution_order,
        test_multi_level_recursion_order,
        test_mixed_files_and_references,
        test_missing_referenced_command,
        test_direct_missing_command,
        test_circular_reference,
        test_self_referential_command,
    ]

    print("=" * 60)
    print("Running CommandRegistry.resolveEnvironment() Tests")
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
