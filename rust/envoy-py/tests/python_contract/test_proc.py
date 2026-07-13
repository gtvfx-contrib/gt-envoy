"""Public-API contract tests for ``envoy.proc``, run against the compiled
``envoy-py`` (PyO3) wheel.

Adapted from ``py/envoy/test_bundle/test_proc.py``: this file keeps only the
tests that exercise the public surface (``envoy.proc.Environment``, the
``call``/``spawn``/``checkCall``/``checkOutput`` free functions, ``PIPE``,
``CalledProcessError``, ``CommandNotFoundError``). Tests for the private
``_loadRegistry``/``_collectEnvFiles``/``_resolveEnvoyExe`` helpers were
intentionally dropped -- see ``tests/python_contract/README.md``.
"""

import json
import os
import sys
from pathlib import Path

import pytest

import envoy.proc as proc
from envoy import CalledProcessError, CommandNotFoundError
from envoy.proc import PIPE, Environment

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _makeBundle(tmp_dir: Path, name: str, commands: dict, env_files: dict) -> Path:
    """Create a minimal bundle directory tree.

    Produces::

        <tmp_dir>/gt/<name>/
            .git/          <- makes isGitRepo() return True
            .envoy/
                commands.json
                <env_file>.json  (one per entry in env_files)

    Returns:
        Path to the bundle root (``<tmp_dir>/gt/<name>``).

    """
    bundle_root = tmp_dir / "gt" / name
    envoy_env = bundle_root / ".envoy"
    envoy_env.mkdir(parents=True)
    # Make it look like a git repo so findGitRepos() picks it up.
    (bundle_root / ".git").mkdir()

    (envoy_env / "commands.json").write_text(json.dumps(commands), encoding="utf-8")
    for filename, content in env_files.items():
        (envoy_env / filename).write_text(json.dumps(content), encoding="utf-8")

    return bundle_root


def _makeCommandsDir(tmp_dir: Path, commands: dict, env_files: dict) -> Path:
    """Create a bare ``.envoy`` directory (no bundle/git structure).

    Returns:
        Path to the ``.envoy/commands.json`` file.

    """
    envoy_env = tmp_dir / ".envoy"
    envoy_env.mkdir(parents=True)

    cf = envoy_env / "commands.json"
    cf.write_text(json.dumps(commands), encoding="utf-8")
    for filename, content in env_files.items():
        (envoy_env / filename).write_text(json.dumps(content), encoding="utf-8")

    return cf


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


# ---------------------------------------------------------------------------
# Environment class
# ---------------------------------------------------------------------------


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
        cf = _pythonCommandsFile(tmp_path)
        env = Environment("py", commands_file=cf)
        proc_obj = env.spawn(["-c", "pass"])
        # envoy-py's spawn() returns a Popen-*like* object (`envoy.proc.PyPopen`)
        # rather than a real `subprocess.Popen` instance -- a known, accepted
        # difference from py/envoy (see README.md). Assert duck-typed
        # compatibility (the actual surface real consumers use) instead of
        # `isinstance(proc_obj, subprocess.Popen)`.
        assert hasattr(proc_obj, "wait")
        assert hasattr(proc_obj, "pid")
        assert hasattr(proc_obj, "returncode")
        proc_obj.wait()

    def test_spawn_nonblocking(self, tmp_path):
        """spawn() returns before the process exits."""
        import time

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
    """Tests for the module-level call / spawn / checkCall / checkOutput.

    Free functions now route every invocation through the envoy CLI.  The
    *cmd* list is passed verbatim as envoy's argument list, so envoy flags
    such as ``-c`` (commands file) can be included directly.
    """

    def test_call_empty_cmd_raises(self):
        with pytest.raises(ValueError, match="non-empty"):
            proc.call([])

    def test_call_success(self, tmp_path):
        cf = _pythonCommandsFile(tmp_path)
        rc = proc.call(["-cf", str(cf), "py", "-c", "pass"])
        assert rc == 0

    def test_call_nonzero(self, tmp_path):
        cf = _pythonCommandsFile(tmp_path)
        rc = proc.call(["-cf", str(cf), "py", "-c", "raise SystemExit(7)"])
        assert rc == 7

    def test_call_pipe_raises(self, tmp_path):
        cf = _pythonCommandsFile(tmp_path)
        with pytest.raises(ValueError, match="PIPE"):
            proc.call(["-cf", str(cf), "py", "-c", "pass"], stdout=PIPE)

    def test_spawn_empty_cmd_raises(self):
        with pytest.raises(ValueError, match="non-empty"):
            proc.spawn([])

    def test_spawn_returns_popen(self, tmp_path):
        cf = _pythonCommandsFile(tmp_path)
        p = proc.spawn(["-cf", str(cf), "py", "-c", "pass"])
        # See TestEnvironmentSpawn.test_spawn_returns_popen: PyPopen is
        # Popen-*compatible* (duck-typed), not `isinstance`-compatible.
        assert hasattr(p, "wait")
        assert hasattr(p, "pid")
        assert hasattr(p, "returncode")
        p.wait()

    def test_check_call_success(self, tmp_path):
        cf = _pythonCommandsFile(tmp_path)
        rc = proc.checkCall(["-cf", str(cf), "py", "-c", "pass"])
        assert rc == 0

    def test_check_call_failure_raises(self, tmp_path):
        cf = _pythonCommandsFile(tmp_path)
        with pytest.raises(CalledProcessError):
            proc.checkCall(["-cf", str(cf), "py", "-c", "raise SystemExit(2)"])

    def test_check_output_empty_cmd_raises(self):
        with pytest.raises(ValueError, match="non-empty"):
            proc.checkOutput([])

    def test_check_output_captures_stdout(self, tmp_path):
        cf = _pythonCommandsFile(tmp_path)
        out = proc.checkOutput(["-cf", str(cf), "py", "-c", "print('envoy_output')"])
        assert b"envoy_output" in out

    def test_check_output_failure_raises(self, tmp_path):
        cf = _pythonCommandsFile(tmp_path)
        with pytest.raises(CalledProcessError):
            proc.checkOutput(["-cf", str(cf), "py", "-c", "raise SystemExit(1)"])

    def test_check_output_stdout_kwarg_raises(self, tmp_path):
        cf = _pythonCommandsFile(tmp_path)
        with pytest.raises(ValueError, match="stdout"):
            proc.checkOutput(["-cf", str(cf), "py", "-c", "pass"], stdout=PIPE)

    def test_call_with_envoy_flags_in_cmd(self, tmp_path):
        """Envoy CLI flags embedded in cmd (e.g. -cf path) are forwarded."""
        cf = _pythonCommandsFile(tmp_path)
        # Use the -cf= equals form to exercise normalisation end-to-end.
        rc = proc.call([f"-cf={cf}", "py", "-c", "pass"])
        assert rc == 0


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
            wrapper.write_text(f'@echo off\n"{sys.executable}" %*\n', encoding="utf-8")
            alias_entry = "${__BUNDLE__}/bin/pyalias.bat"
        else:
            wrapper = bundle_bin / "pyalias.sh"
            wrapper.write_text(f'#!/bin/sh\nexec "{sys.executable}" "$@"\n', encoding="utf-8")
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
