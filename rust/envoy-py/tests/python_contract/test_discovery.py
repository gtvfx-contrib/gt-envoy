"""Public-API contract tests for bundle discovery, run against the compiled
``envoy-py`` (PyO3) wheel.

Adapted from ``py/envoy/test_bundle/test_discovery.py``: that file's
``test_validation`` used the private ``envoy._discovery.validateBundle``
helper (no public equivalent) and relied on a fixed ``examples/`` fixture
directory that doesn't exist at this location, so it was dropped. The
``loadBundlesFromConfig``/``getBundles`` coverage is preserved here as real
assertions (rather than the original's try/except-and-print smoke checks)
against self-contained ``tmp_path`` fixtures, using only the public
``envoy.loadBundlesFromConfig``/``envoy.getBundles`` surface.
"""

import json
from pathlib import Path

import envoy


def _makeBundle(tmp_dir: Path, name: str) -> Path:
    """Create a minimal bundle directory tree (git repo + .envoy marker)."""
    bundle_root = tmp_dir / "gt" / name
    envoy_env = bundle_root / ".envoy"
    envoy_env.mkdir(parents=True)
    (bundle_root / ".git").mkdir()
    (envoy_env / "commands.json").write_text("{}", encoding="utf-8")
    return bundle_root


def test_config_loading(tmp_path):
    """loadBundlesFromConfig() resolves bundle specs listed in a config file."""
    bundle_root = _makeBundle(tmp_path, "myapp")

    config_file = tmp_path / "bundles.json"
    config_file.write_text(
        json.dumps({"bundles": [str(bundle_root)]}),
        encoding="utf-8",
    )

    bundles = envoy.loadBundlesFromConfig(config_file)

    assert len(bundles) == 1
    assert bundles[0].name == "myapp"


def test_auto_discovery(tmp_path, monkeypatch):
    """getBundles() auto-discovers bundles under ENVOY_BNDL_ROOTS."""
    _makeBundle(tmp_path, "autodiscovered")
    monkeypatch.setenv("ENVOY_BNDL_ROOTS", str(tmp_path))

    bundles = envoy.getBundles()

    assert any(bundle.name == "autodiscovered" for bundle in bundles)
