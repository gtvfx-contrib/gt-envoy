"""Public-API contract tests for bundle discovery, run against the compiled
``envoy-py`` (PyO3) wheel.

Adapted from ``py/envoy/test_bundle/test_discovery.py``: that file's
``test_validation`` used the private ``envoy._discovery.validateBundle``
helper (no public equivalent) and relied on a fixed ``examples/`` fixture
directory that doesn't exist at this location, so it was dropped. The
``loadBundlesFromStack``/``getBundles`` coverage is preserved here as real
assertions (rather than the original's try/except-and-print smoke checks)
against self-contained ``tmp_path`` fixtures, using only the public
``envoy.loadBundlesFromStack``/``envoy.getBundles`` surface.
"""

from pathlib import Path

import envoy


def testLegacyStackAndCacheNamesAreNotExported():
    """Legacy domain names are absent from the clean-break Python API."""
    legacy_names = (
        "BundleConfig",
        "Pipeline",
        "PipelineConfig",
        "PackageCache",
        "getCurrentBundleConfig",
        "getCurrentPipeline",
        "loadBundlesFromConfig",
    )

    assert all(not hasattr(envoy, name) for name in legacy_names)


def _makeBundle(tmp_dir: Path, name: str) -> Path:
    """Create a minimal bundle directory tree (git repo + .envoy marker)."""
    bundle_root = tmp_dir / "gt" / name
    envoy_env = bundle_root / ".envoy"
    envoy_env.mkdir(parents=True)
    (bundle_root / ".git").mkdir()
    (envoy_env / "commands.json").write_text("{}", encoding="utf-8")
    return bundle_root


def test_stack_loading(tmp_path):
    """Stack and loadBundlesFromStack() resolve bundle paths from YAML."""
    bundle_root = _makeBundle(tmp_path, "myapp")

    stack_file = tmp_path / "studio.estack"
    stack_file.write_text(
        "\n".join(
            [
                "name: studio",
                "namespace: gt:tools",
                "metadata:",
                "  owner: runtime",
                "bundles:",
                f"  - path: '{bundle_root}'",
                "    metadata:",
                "      role: core",
            ]
        ),
        encoding="utf-8",
    )

    stack = envoy.Stack(stack_file)
    bundles = envoy.loadBundlesFromStack(stack_file)

    assert stack.name == "studio"
    assert stack.namespace == "gt:tools"
    assert stack.path == stack_file.resolve()
    assert stack.source == {"type": "local", "path": stack_file.resolve()}
    assert stack.pinned_version is None
    assert stack.registry_version is None
    assert stack.metadata == {"owner": "runtime"}
    assert stack.commands == []
    assert len(bundles) == 1
    assert bundles[0].name == "myapp"


def test_auto_discovery(tmp_path, monkeypatch):
    """getBundles() auto-discovers bundles under ENVOY_BNDL_ROOTS."""
    _makeBundle(tmp_path, "autodiscovered")
    monkeypatch.setenv("ENVOY_BNDL_ROOTS", str(tmp_path))

    bundles = envoy.getBundles()

    assert any(bundle.name == "autodiscovered" for bundle in bundles)
