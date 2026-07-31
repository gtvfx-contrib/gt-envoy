"""Tests for Envoy release automation."""

from __future__ import annotations

import importlib.util
from pathlib import Path
from unittest import mock

import pytest

SCRIPT_PATH = Path(__file__).parents[1] / "scripts" / "release_automation.py"
SPEC = importlib.util.spec_from_file_location("release_automation", SCRIPT_PATH)
assert SPEC is not None and SPEC.loader is not None
release_automation = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release_automation)


@pytest.mark.parametrize(
    "version",
    ["0.6.0", "1.0.0-rc.1", "2.3.4+build.5"],
)
def testValidateVersionAcceptsSemver(version):
    """Valid SemVer values are returned unchanged."""
    assert release_automation.validateVersion(version) == version


@pytest.mark.parametrize("version", ["v0.6.0", "01.2.3", "1.2", "latest"])
def testValidateVersionRejectsInvalidValues(version):
    """Invalid release values are rejected."""
    with pytest.raises(ValueError, match="Invalid semantic version"):
        release_automation.validateVersion(version)


def testReplaceWorkspaceVersionTargetsOnlyWorkspacePackage(tmp_path):
    """The workspace version replacement leaves dependency versions alone."""
    manifest_path = tmp_path / "Cargo.toml"
    manifest_path.write_text(
        '[workspace.package]\nversion = "0.5.1"\n\n[workspace.dependencies]\nitem = "1"\n',
        encoding="utf-8",
    )
    release_automation.replaceWorkspaceVersion(manifest_path, "0.6.0")
    assert manifest_path.read_text(encoding="utf-8") == (
        '[workspace.package]\nversion = "0.6.0"\n\n[workspace.dependencies]\nitem = "1"\n'
    )


def testPrepareReleaseRefreshesAndChecksLockfile(tmp_path):
    """Preparation asks Cargo to refresh the lockfile before validation."""
    rust_root = tmp_path / "rust"
    rust_root.mkdir()
    (rust_root / "Cargo.toml").write_text(
        '[workspace.package]\nversion = "0.5.1"\n',
        encoding="utf-8",
    )
    with (
        mock.patch.object(release_automation.subprocess, "run") as run_process,
        mock.patch.object(release_automation, "checkRelease") as check_release,
    ):
        release_automation.prepareRelease(tmp_path, "0.6.0")
    run_process.assert_called_once_with(
        ["cargo", "check", "--workspace", "--exclude", "envoy-py"],
        cwd=rust_root,
        check=True,
    )
    check_release.assert_called_once_with(tmp_path, "0.6.0")


def testReplaceUtilsDependencyUsesLocalEnvoyCore(tmp_path):
    """Compatibility testing removes the exact remote pin in its temporary copy."""
    manifest_path = tmp_path / "Cargo.toml"
    manifest_path.write_text(
        '[workspace.dependencies]\n'
        'envoy-core = { git = "https://example.test/envoy", tag = "v0.5.1", '
        'version = "=0.5.1" }\n',
        encoding="utf-8",
    )
    envoy_root = tmp_path / "envoy"
    release_automation.replaceUtilsDependency(manifest_path, envoy_root)
    assert 'envoy-core = { path = "' in manifest_path.read_text(encoding="utf-8")
    assert "tag =" not in manifest_path.read_text(encoding="utf-8")
