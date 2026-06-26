"""Tests for the CLI module."""

import json
import sys
import io
import tempfile
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent.parent.parent.parent))

from envoy._commands import CommandRegistry
from envoy._cli import showCommandInfo, showWhich, _normalizeArgv, runCommand


def _makeRegistry(commands: dict) -> CommandRegistry:
    """Create a CommandRegistry from a dict of command definitions."""
    with tempfile.TemporaryDirectory() as tmpdir:
        f = Path(tmpdir) / "commands.json"
        f.write_text(json.dumps(commands))
        registry = CommandRegistry(f)
    return registry


def test_show_command_info_resolves_inherited_environment(capsys):
    """showCommandInfo should display the fully resolved env file list."""
    with tempfile.TemporaryDirectory() as tmpdir:
        cmds = {
            "base": {"environment": ["base_env.json"]},
            "derived": {"environment": ["base", "derived_env.json"], "alias": ["derived_prog"]},
        }
        f = Path(tmpdir) / "commands.json"
        f.write_text(json.dumps(cmds))
        registry = CommandRegistry(f)

    rc = showCommandInfo(registry, "derived")
    captured = capsys.readouterr()

    assert rc == 0
    # Inherited file should be expanded, not shown as raw reference
    assert "base_env.json" in captured.out
    assert "derived_env.json" in captured.out
    # The raw command-name reference must NOT appear as an entry
    lines = [ln.strip() for ln in captured.out.splitlines()]
    assert "- base" not in lines


def test_show_command_info_wrappererror_returns_1(capsys):
    """showCommandInfo should return 1 and print to stderr on WrapperError."""
    with tempfile.TemporaryDirectory() as tmpdir:
        # "broken" references a command that does not exist
        cmds = {"broken": {"environment": ["nonexistent_cmd"]}}
        f = Path(tmpdir) / "commands.json"
        f.write_text(json.dumps(cmds))
        registry = CommandRegistry(f)

    rc = showCommandInfo(registry, "broken")
    captured = capsys.readouterr()

    assert rc == 1
    assert "Error" in captured.err
    assert "broken" in captured.err


def test_show_which_alias_prints_source_file(capsys):
    """showWhich for an alias command should print the commands.json path."""
    with tempfile.TemporaryDirectory() as tmpdir:
        cmds = {
            "python": {"environment": [], "alias": ["python"]},
        }
        f = Path(tmpdir) / "commands.json"
        f.write_text(json.dumps(cmds))
        registry = CommandRegistry(f)

    rc = showWhich(registry, "python")
    captured = capsys.readouterr()

    assert rc == 0
    assert "aliased to: python" in captured.out
    assert "defined in:" in captured.out


def test_show_which_unknown_command_returns_1(capsys):
    """showWhich should return 1 when the command is not found."""
    with tempfile.TemporaryDirectory() as tmpdir:
        cmds = {"python": {"environment": [], "alias": ["python"]}}
        f = Path(tmpdir) / "commands.json"
        f.write_text(json.dumps(cmds))
        registry = CommandRegistry(f)

    rc = showWhich(registry, "nonexistent")
    captured = capsys.readouterr()

    assert rc == 1
    assert "not found" in captured.err


# ---------------------------------------------------------------------------
# _normalizeArgv tests
# ---------------------------------------------------------------------------

class TestNormalizeArgv:
    """Tests for the _normalizeArgv pre-processing helper."""

    # --- short options ---

    def test_expands_short_env_option(self):
        """-e=python expands to ['-e', 'python']."""
        assert _normalizeArgv(['-e=python', 'cmd']) == ['-e', 'python', 'cmd']

    def test_expands_short_bundles_config_option(self):
        """-bc=studio expands to ['-bc', 'studio']."""
        assert _normalizeArgv(['-bc=studio.json']) == ['-bc', 'studio.json']

    def test_expands_short_set_config_option(self):
        """-sc=KEY=VALUE expands to ['-sc', 'KEY=VALUE'] (splits only on first =)."""
        assert _normalizeArgv(['-sc=bundles_config=/path/x']) == [
            '-sc', 'bundles_config=/path/x'
        ]

    def test_expands_short_commands_file_option(self):
        """-cf=/some/path expands to ['-cf', '/some/path']."""
        assert _normalizeArgv(['-cf=/some/path']) == ['-cf', '/some/path']

    def test_expands_short_get_config_option(self):
        """-gc=key expands to ['-gc', 'key']."""
        assert _normalizeArgv(['-gc=verbosity']) == ['-gc', 'verbosity']

    def test_expands_unknown_short_option(self):
        """Any -x=val short option (not just known ones) is expanded."""
        assert _normalizeArgv(['-x=val']) == ['-x', 'val']

    # --- long options ---

    def test_expands_long_env_option(self):
        """--env=python expands to ['--env', 'python']."""
        assert _normalizeArgv(['--env=python']) == ['--env', 'python']

    def test_expands_long_bundles_config_option(self):
        """--bundles-config=path expands to ['--bundles-config', 'path']."""
        assert _normalizeArgv(['--bundles-config=studio.json']) == [
            '--bundles-config', 'studio.json'
        ]

    def test_expands_long_set_config_option(self):
        """--set-config=KEY=VALUE expands to ['--set-config', 'KEY=VALUE']."""
        assert _normalizeArgv(['--set-config=bundles_config=/path']) == [
            '--set-config', 'bundles_config=/path'
        ]

    # --- flags without values are left unchanged ---

    def test_leaves_boolean_short_flag_unchanged(self):
        """-v (no =) is left unchanged."""
        assert _normalizeArgv(['-v']) == ['-v']

    def test_leaves_boolean_long_flag_unchanged(self):
        """--verbose (no =) is left unchanged."""
        assert _normalizeArgv(['--verbose']) == ['--verbose']

    # --- stops at the command positional ---

    def test_does_not_split_post_command_arg(self):
        """Tokens after the command positional are never split."""
        result = _normalizeArgv(['-e=python', 'code.cmd', '-X=option'])
        assert result == ['-e', 'python', 'code.cmd', '-X=option']

    def test_does_not_split_post_command_long_arg(self):
        """Long-option tokens after the command positional are never split."""
        result = _normalizeArgv(['-b=studio', 'maya', '--scene=shot.ma'])
        assert result == ['-b', 'studio', 'maya', '--scene=shot.ma']

    # --- combined and edge cases ---

    def test_multiple_options_expanded(self):
        """Multiple options in a single argv are all expanded."""
        result = _normalizeArgv(['-b=studio', '-e=python', 'code.cmd'])
        assert result == ['-b', 'studio', '-e', 'python', 'code.cmd']

    def test_empty_list_unchanged(self):
        """Empty list is returned unchanged."""
        assert _normalizeArgv([]) == []

    def test_double_dash_end_of_options_unchanged(self):
        """-- (end-of-options marker) is left as-is and does not stop normalisation."""
        result = _normalizeArgv(['--', '-e=python', 'cmd'])
        assert result == ['--', '-e', 'python', 'cmd']


# ---------------------------------------------------------------------------
# runCommand raw-path tests
# ---------------------------------------------------------------------------

class TestRunCommandRawPath:
    """Tests for runCommand when the command is a raw executable path."""

    def test_absolute_path_runs_without_registry(self):
        """An absolute path executes successfully even with an empty registry."""
        registry = CommandRegistry()
        rc = runCommand(
            registry=registry,
            command_name=sys.executable,
            args=['-c', 'pass'],
        )
        assert rc == 0

    def test_absolute_path_nonzero_exit(self):
        """Non-zero exit is returned correctly for raw paths."""
        registry = CommandRegistry()
        rc = runCommand(
            registry=registry,
            command_name=sys.executable,
            args=['-c', 'raise SystemExit(5)'],
        )
        assert rc == 5

    def test_raw_path_with_env_override_validates_override(self, capsys):
        """env_override must exist in the registry; error is printed if not."""
        registry = CommandRegistry()
        rc = runCommand(
            registry=registry,
            command_name=sys.executable,
            args=['-c', 'pass'],
            env_override='no_such_cmd',
        )
        captured = capsys.readouterr()
        assert rc == 1
        assert 'no_such_cmd' in captured.err