"""Tests for the envoy.proc public API.

Covers:
- Bundle discovery integration (_loadRegistry)
- Environment file collection including command-reference resolution
- Environment class: build (idempotent), call, spawn, checkCall, checkOutput
- Free functions: call, spawn, checkCall, checkOutput
- Error paths: CommandNotFoundError, CalledProcessError, ValueError
"""
import json
import os
import sys
import tempfile
from pathlib import Path

import pytest

# Ensure the package root is importable regardless of how pytest is invoked.
sys.path.insert(0, str(Path(__file__).parent.parent.parent.parent.parent))

import envoy.proc as proc
from envoy.proc import (
    Environment,
    _loadRegistry,
    _collectEnvFiles,
    PIPE,
)
from envoy._exceptions import (
    CalledProcessError,
    CommandNotFoundError,
    EnvironmentBuildError,
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _makeBundle(tmp_dir: Path, name: str, commands: dict, env_files: dict) -> Path:
    """Create a minimal bundle directory tree.

    Produces::

        <tmp_dir>/<ns>/<name>/
            .git/          <- makes isGitRepo() return True
            envoy_env/
                commands.json
                <env_file>.json  (one per entry in env_files)

    Args:
        tmp_dir:   Temp root (the ENVOY_BNDL_ROOTS value should point here).
        name:      Bundle name (also used as namespace for simplicity).
        commands:  Dict written as commands.json.
        env_files: Mapping of filename -> dict (written as JSON env files).

    Returns:
        Path to the bundle root (``<tmp_dir>/gt/<name>``).
    """
    bundle_root = tmp_dir / "gt" / name
    envoy_env = bundle_root / "envoy_env"
    envoy_env.mkdir(parents=True)
    # Make it look like a git repo so findGitRepos() picks it up.
    (bundle_root / ".git").mkdir()

    (envoy_env / "commands.json").write_text(json.dumps(commands), encoding="utf-8")
    for filename, content in env_files.items():
        (envoy_env / filename).write_text(json.dumps(content), encoding="utf-8")

    return bundle_root


def _makeCommandsDir(tmp_dir: Path, commands: dict, env_files: dict) -> Path:
    """Create a bare envoy_env directory (no bundle/git structure).

    Returns:
        Path to the ``envoy_env/commands.json`` file.
    """
    envoy_env = tmp_dir / "envoy_env"
    envoy_env.mkdir(parents=True)

    cf = envoy_env / "commands.json"
    cf.write_text(json.dumps(commands), encoding="utf-8")
    for filename, content in env_files.items():
        (envoy_env / filename).write_text(json.dumps(content), encoding="utf-8")

    return cf


# ---------------------------------------------------------------------------
# _loadRegistry
# ---------------------------------------------------------------------------

class TestLoadRegistry:
    """Tests for the _loadRegistry helper."""

    def test_from_commands_file(self, tmp_path):
        """Registry is populated when a bare commands.json is provided."""
        cf = _makeCommandsDir(
            tmp_path,
            commands={"mytool": {"environment": ["mytool_env.json"]}},
            env_files={"mytool_env.json": {"MY_VAR": "hello"}},
        )

        registry, bundles = _loadRegistry(commands_file=cf)

        assert "mytool" in registry
        assert bundles is None  # bare commands.json → no bundle list

    def test_from_bundle_roots(self, tmp_path):
        """Registry is populated from bundle roots with git repos."""
        _makeBundle(
            tmp_path,
            name="myapp",
            commands={"myapp": {"environment": ["myapp_env.json"]}},
            env_files={"myapp_env.json": {"APP_VAR": "1"}},
        )

        registry, bundles = _loadRegistry(bundle_roots=[str(tmp_path)])

        assert "myapp" in registry
        assert bundles is not None
        assert len(bundles) >= 1

    def test_commands_file_preferred_when_bundle_roots_empty(self, tmp_path):
        """Falls back to commands_file when bundle_roots yields nothing."""
        cf = _makeCommandsDir(
            tmp_path / "bare",
            commands={"fallback_cmd": {"environment": ["fb_env.json"]}},
            env_files={"fb_env.json": {"FB": "1"}},
        )

        # Provide an empty bundle_roots list so bundle discovery yields nothing.
        registry, bundles = _loadRegistry(bundle_roots=[], commands_file=cf)

        assert "fallback_cmd" in registry
        assert bundles is None

    def test_unknown_bundle_root_returns_empty_registry(self, tmp_path):
        """Non-existent bundle root produces an empty registry (no crash)."""
        nonexistent = tmp_path / "no_such_dir"
        registry, bundles = _loadRegistry(bundle_roots=[str(nonexistent)])

        assert len(registry) == 0


# ---------------------------------------------------------------------------
# _collectEnvFiles
# ---------------------------------------------------------------------------

class TestCollectEnvFiles:
    """Tests for the _collectEnvFiles helper."""

    def test_collects_env_files_from_commands_json(self, tmp_path):
        """Env files are resolved from a bare commands.json (non-bundle mode)."""
        cf = _makeCommandsDir(
            tmp_path,
            commands={"tool": {"environment": ["tool_env.json"]}},
            env_files={"tool_env.json": {"TOOL_VAR": "x"}},
        )

        registry, bundles = _loadRegistry(commands_file=cf)
        env_files = _collectEnvFiles("tool", registry, bundles)

        assert any(Path(p).name == "tool_env.json" for p in env_files)

    def test_command_not_found_raises(self, tmp_path):
        """CommandNotFoundError when the command is not in the registry."""
        cf = _makeCommandsDir(
            tmp_path,
            commands={"tool": {"environment": ["tool_env.json"]}},
            env_files={"tool_env.json": {"TOOL_VAR": "x"}},
        )

        registry, bundles = _loadRegistry(commands_file=cf)

        with pytest.raises(CommandNotFoundError):
            _collectEnvFiles("no_such_command", registry, bundles)

    def test_global_env_included_from_bundle(self, tmp_path):
        """global_env.json is prepended when present in a bundle."""
        _makeBundle(
            tmp_path,
            name="myapp",
            commands={"myapp": {"environment": ["myapp_env.json"]}},
            env_files={
                "global_env.json": {"GLOBAL_VAR": "global"},
                "myapp_env.json": {"APP_VAR": "app"},
            },
        )

        registry, bundles = _loadRegistry(bundle_roots=[str(tmp_path)])
        env_files = _collectEnvFiles("myapp", registry, bundles)

        names = [Path(p).name for p in env_files]
        assert "global_env.json" in names
        assert "myapp_env.json" in names
        assert names.index("global_env.json") < names.index("myapp_env.json")

    def test_command_reference_resolved(self, tmp_path):
        """Environment entries that name another command are spliced in recursively."""
        cf = _makeCommandsDir(
            tmp_path,
            commands={
                "base": {"environment": ["base_env.json"]},
                "derived": {"environment": ["base", "derived_env.json"]},
            },
            env_files={
                "base_env.json": {"BASE_VAR": "base"},
                "derived_env.json": {"DERIVED_VAR": "derived"},
            },
        )

        registry, bundles = _loadRegistry(commands_file=cf)
        env_files = _collectEnvFiles("derived", registry, bundles)

        names = [Path(p).name for p in env_files]
        assert "base_env.json" in names
        assert "derived_env.json" in names
        # base must appear before derived
        assert names.index("base_env.json") < names.index("derived_env.json")


# ---------------------------------------------------------------------------
# Environment class
# ---------------------------------------------------------------------------

def _pythonCommandsFile(tmp_path: Path) -> Path:
    """Return a commands.json that defines a 'py' command using ``python``."""
    return _makeCommandsDir(
        tmp_path,
        commands={
            "py": {
                "environment": ["py_env.json"],
                "alias": [sys.executable],
            }
        },
        env_files={"py_env.json": {"ENVOY_TEST_MARKER": "proc_test"}},
    )


class TestEnvironmentBuild:
    """Tests for Environment.build()."""

    def test_build_returns_env_dict(self, tmp_path):
        """build() returns a dict containing variables from the env file."""
        cf = _pythonCommandsFile(tmp_path)
        env = Environment("py", commands_file=cf)
        result = env.build()

        assert isinstance(result, dict)
        assert result.get("ENVOY_TEST_MARKER") == "proc_test"

    def test_build_is_idempotent(self, tmp_path):
        """Calling build() twice returns the same object (no re-parse)."""
        cf = _pythonCommandsFile(tmp_path)
        env = Environment("py", commands_file=cf)

        first = env.build()
        second = env.build()

        assert first is second

    def test_build_unknown_command_raises(self, tmp_path):
        """CommandNotFoundError when the command does not exist."""
        cf = _pythonCommandsFile(tmp_path)
        env = Environment("no_such_cmd", commands_file=cf)

        with pytest.raises(CommandNotFoundError):
            env.build()


class TestEnvironmentProperties:
    """Tests for Environment properties and repr."""

    def test_command_property(self, tmp_path):
        cf = _pythonCommandsFile(tmp_path)
        env = Environment("py", commands_file=cf)
        assert env.command == "py"

    def test_allowlist_property(self, tmp_path):
        cf = _pythonCommandsFile(tmp_path)
        env = Environment("py", allowlist=["MY_VAR"], commands_file=cf)
        assert "MY_VAR" in env.allowlist

    def test_whitelist_alias(self, tmp_path):
        """whitelist is a deprecated alias that maps to allowlist."""
        cf = _pythonCommandsFile(tmp_path)
        env = Environment("py", whitelist=["LEGACY_VAR"], commands_file=cf)
        assert "LEGACY_VAR" in env.whitelist
        assert "LEGACY_VAR" in env.allowlist

    def test_repr(self, tmp_path):
        cf = _pythonCommandsFile(tmp_path)
        env = Environment("py", commands_file=cf)
        assert "py" in str(env)
        assert "py" in repr(env)


class TestEnvironmentCall:
    """Tests for Environment.call()."""

    def test_call_returns_zero_on_success(self, tmp_path):
        cf = _pythonCommandsFile(tmp_path)
        env = Environment("py", commands_file=cf)
        rc = env.call(["-c", "pass"])
        assert rc == 0

    def test_call_returns_nonzero_on_failure(self, tmp_path):
        cf = _pythonCommandsFile(tmp_path)
        env = Environment("py", commands_file=cf)
        rc = env.call(["-c", "raise SystemExit(42)"])
        assert rc == 42

    def test_call_pipe_stdout_raises(self, tmp_path):
        """call() raises ValueError when stdout=PIPE is requested."""
        cf = _pythonCommandsFile(tmp_path)
        env = Environment("py", commands_file=cf)
        with pytest.raises(ValueError, match="PIPE"):
            env.call(["-c", "pass"], stdout=PIPE)

    def test_call_pipe_stderr_raises(self, tmp_path):
        """call() raises ValueError when stderr=PIPE is requested."""
        cf = _pythonCommandsFile(tmp_path)
        env = Environment("py", commands_file=cf)
        with pytest.raises(ValueError, match="PIPE"):
            env.call(["-c", "pass"], stderr=PIPE)


class TestEnvironmentCheckCall:
    """Tests for Environment.checkCall()."""

    def test_check_call_success(self, tmp_path):
        cf = _pythonCommandsFile(tmp_path)
        env = Environment("py", commands_file=cf)
        rc = env.checkCall(["-c", "pass"])
        assert rc == 0

    def test_check_call_failure_raises(self, tmp_path):
        cf = _pythonCommandsFile(tmp_path)
        env = Environment("py", commands_file=cf)
        with pytest.raises(CalledProcessError) as exc_info:
            env.checkCall(["-c", "raise SystemExit(1)"])
        assert exc_info.value.returncode == 1


class TestEnvironmentCheckOutput:
    """Tests for Environment.checkOutput()."""

    def test_check_output_captures_stdout(self, tmp_path):
        cf = _pythonCommandsFile(tmp_path)
        env = Environment("py", commands_file=cf)
        out = env.checkOutput(["-c", "print('hello_envoy')"])
        assert b"hello_envoy" in out

    def test_check_output_failure_raises(self, tmp_path):
        cf = _pythonCommandsFile(tmp_path)
        env = Environment("py", commands_file=cf)
        with pytest.raises(CalledProcessError):
            env.checkOutput(["-c", "raise SystemExit(1)"])

    def test_check_output_stdout_kwarg_raises(self, tmp_path):
        """Passing stdout= to checkOutput raises ValueError."""
        cf = _pythonCommandsFile(tmp_path)
        env = Environment("py", commands_file=cf)
        with pytest.raises(ValueError, match="stdout"):
            env.checkOutput(["-c", "pass"], stdout=PIPE)

    def test_check_output_input_and_stdin_raises(self, tmp_path):
        """Passing both input= and stdin= to checkOutput raises ValueError."""
        import subprocess
        cf = _pythonCommandsFile(tmp_path)
        env = Environment("py", commands_file=cf)
        with pytest.raises(ValueError, match="input"):
            env.checkOutput(
                ["-c", "pass"],
                input=b"",
                stdin=subprocess.DEVNULL,
            )

    def test_check_output_with_input(self, tmp_path):
        """bytes passed via input= are forwarded to the process stdin."""
        cf = _pythonCommandsFile(tmp_path)
        env = Environment("py", commands_file=cf)
        out = env.checkOutput(
            ["-c", "import sys; print(sys.stdin.read().strip())"],
            input=b"piped_data",
        )
        assert b"piped_data" in out


class TestEnvironmentSpawn:
    """Tests for Environment.spawn()."""

    def test_spawn_returns_popen(self, tmp_path):
        import subprocess
        cf = _pythonCommandsFile(tmp_path)
        env = Environment("py", commands_file=cf)
        proc_obj = env.spawn(["-c", "pass"])
        assert isinstance(proc_obj, subprocess.Popen)
        proc_obj.wait()

    def test_spawn_nonblocking(self, tmp_path):
        """spawn() returns before the process exits."""
        import time
        import subprocess
        cf = _pythonCommandsFile(tmp_path)
        env = Environment("py", commands_file=cf)
        start = time.monotonic()
        p = env.spawn(["-c", "import time; time.sleep(0.3)"])
        elapsed = time.monotonic() - start
        # spawn must return faster than the child's sleep
        assert elapsed < 0.3, "spawn() should return before the process finishes"
        p.wait()

    def test_spawn_env_variable_visible_in_child(self, tmp_path):
        """Variables from the env file are visible inside the spawned process."""
        script = (
            "import os, sys; "
            "sys.exit(0 if os.environ.get('ENVOY_TEST_MARKER') == 'proc_test' else 1)"
        )
        cf = _pythonCommandsFile(tmp_path)
        env = Environment("py", commands_file=cf)
        p = env.spawn(["-c", script])
        p.wait()
        assert p.returncode == 0


# ---------------------------------------------------------------------------
# Free functions
# ---------------------------------------------------------------------------

class TestProcFreeFunctions:
    """Tests for the module-level call / spawn / checkCall / checkOutput."""

    def test_call_empty_cmd_raises(self):
        with pytest.raises(ValueError, match="non-empty"):
            proc.call([])

    def test_call_success(self, tmp_path):
        cf = _pythonCommandsFile(tmp_path)
        rc = proc.call(["py", "-c", "pass"], commands_file=cf)
        assert rc == 0

    def test_call_nonzero(self, tmp_path):
        cf = _pythonCommandsFile(tmp_path)
        rc = proc.call(["py", "-c", "raise SystemExit(7)"], commands_file=cf)
        assert rc == 7

    def test_call_pipe_raises(self, tmp_path):
        cf = _pythonCommandsFile(tmp_path)
        with pytest.raises(ValueError, match="PIPE"):
            proc.call(["py", "-c", "pass"], commands_file=cf, stdout=PIPE)

    def test_spawn_empty_cmd_raises(self):
        with pytest.raises(ValueError, match="non-empty"):
            proc.spawn([])

    def test_spawn_returns_popen(self, tmp_path):
        import subprocess
        cf = _pythonCommandsFile(tmp_path)
        p = proc.spawn(["py", "-c", "pass"], commands_file=cf)
        assert isinstance(p, subprocess.Popen)
        p.wait()

    def test_check_call_success(self, tmp_path):
        cf = _pythonCommandsFile(tmp_path)
        rc = proc.checkCall(["py", "-c", "pass"], commands_file=cf)
        assert rc == 0

    def test_check_call_failure_raises(self, tmp_path):
        cf = _pythonCommandsFile(tmp_path)
        with pytest.raises(CalledProcessError):
            proc.checkCall(["py", "-c", "raise SystemExit(2)"], commands_file=cf)

    def test_check_output_empty_cmd_raises(self):
        with pytest.raises(ValueError, match="non-empty"):
            proc.checkOutput([])

    def test_check_output_captures_stdout(self, tmp_path):
        cf = _pythonCommandsFile(tmp_path)
        out = proc.checkOutput(["py", "-c", "print('envoy_output')"], commands_file=cf)
        assert b"envoy_output" in out

    def test_check_output_failure_raises(self, tmp_path):
        cf = _pythonCommandsFile(tmp_path)
        with pytest.raises(CalledProcessError):
            proc.checkOutput(["py", "-c", "raise SystemExit(1)"], commands_file=cf)

    def test_check_output_stdout_kwarg_raises(self, tmp_path):
        cf = _pythonCommandsFile(tmp_path)
        with pytest.raises(ValueError, match="stdout"):
            proc.checkOutput(["py", "-c", "pass"], commands_file=cf, stdout=PIPE)


# ---------------------------------------------------------------------------
# Bundle discovery integration
# ---------------------------------------------------------------------------

class TestBundleDiscoveryIntegration:
    """End-to-end tests exercising bundle discovery + environment building."""

    def test_environment_built_from_bundle_roots(self, tmp_path):
        """Environment variables from a bundle env file reach the subprocess."""
        _makeBundle(
            tmp_path,
            name="myapp",
            commands={
                "myapp": {
                    "environment": ["myapp_env.json"],
                    "alias": [sys.executable],
                }
            },
            env_files={"myapp_env.json": {"MYAPP_BUNDLE_VAR": "bundle_value"}},
        )

        env = Environment("myapp", bundle_roots=[str(tmp_path)])
        built = env.build()
        assert built.get("MYAPP_BUNDLE_VAR") == "bundle_value"

    def test_inherited_command_environment(self, tmp_path):
        """A command that references another command gets both env files applied."""
        cf = _makeCommandsDir(
            tmp_path,
            commands={
                "base": {"environment": ["base_env.json"]},
                "derived": {
                    "environment": ["base", "derived_env.json"],
                    "alias": [sys.executable],
                },
            },
            env_files={
                "base_env.json": {"BASE_INHERITED": "yes"},
                "derived_env.json": {"DERIVED_OWN": "yes"},
            },
        )

        env = Environment("derived", commands_file=cf)
        built = env.build()

        assert built.get("BASE_INHERITED") == "yes"
        assert built.get("DERIVED_OWN") == "yes"

    def test_global_env_applied_from_bundle(self, tmp_path):
        """global_env.json from a bundle is included in the built environment."""
        _makeBundle(
            tmp_path,
            name="myapp",
            commands={
                "myapp": {
                    "environment": ["myapp_env.json"],
                    "alias": [sys.executable],
                }
            },
            env_files={
                "global_env.json": {"GLOBAL_BUNDLE_VAR": "from_global"},
                "myapp_env.json": {"APP_VAR": "from_app"},
            },
        )

        env = Environment("myapp", bundle_roots=[str(tmp_path)])
        built = env.build()

        assert built.get("GLOBAL_BUNDLE_VAR") == "from_global"
        assert built.get("APP_VAR") == "from_app"

    def test_bundle_alias_expansion(self, tmp_path):
        """${__BUNDLE__} in an alias is expanded to the bundle root before execution."""
        # Create a thin wrapper script inside the bundle that delegates to the
        # real Python interpreter.  This lets us reference it as
        # "${__BUNDLE__}/bin/<script>" without copying the full runtime.
        bundle_bin = tmp_path / "gt" / "myapp" / "bin"
        bundle_bin.mkdir(parents=True)

        if os.name == "nt":
            wrapper = bundle_bin / "pyalias.bat"
            wrapper.write_text(
                f'@echo off\n"{sys.executable}" %*\n', encoding="utf-8"
            )
            alias_entry = "${__BUNDLE__}/bin/pyalias.bat"
        else:
            wrapper = bundle_bin / "pyalias.sh"
            wrapper.write_text(
                f'#!/bin/sh\nexec "{sys.executable}" "$@"\n', encoding="utf-8"
            )
            wrapper.chmod(0o755)
            alias_entry = "${__BUNDLE__}/bin/pyalias.sh"

        _makeBundle(
            tmp_path,
            name="myapp",
            commands={
                "myapp": {
                    "environment": ["myapp_env.json"],
                    "alias": [alias_entry],
                }
            },
            env_files={"myapp_env.json": {"BUNDLE_ALIAS_TEST": "expanded"}},
        )

        env = Environment("myapp", bundle_roots=[str(tmp_path)])
        # If ${__BUNDLE__} is NOT expanded, EnvironmentBuildError is raised.
        # If it IS expanded, the wrapper runs Python successfully.
        result = env.checkOutput(
            ["-c", "import os; print(os.environ['BUNDLE_ALIAS_TEST'])"],
        )
        assert result.strip() == b"expanded"


if __name__ == "__main__":
    sys.exit(pytest.main([__file__, "-v"]))
