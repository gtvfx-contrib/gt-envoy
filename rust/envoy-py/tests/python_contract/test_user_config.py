"""Public Python contracts for Envoy's shared config root."""

import json

import envoy


def testGetConfigRootHonorsEnvironmentOverride(monkeypatch, tmp_path):
    """The root API resolves a non-empty override at call time."""
    config_root = tmp_path / "shared-config"
    monkeypatch.setenv("ENVOY_CONFIG_ROOT", str(config_root))

    assert envoy.getConfigRoot() == config_root


def testLoadUserConfigUsesEffectiveRoot(monkeypatch, tmp_path):
    """Default user-config persistence uses the effective shared root."""
    config_root = tmp_path / "shared-config"
    monkeypatch.setenv("ENVOY_CONFIG_ROOT", str(config_root))
    user_config = envoy.loadUserConfig()

    user_config.set("stack", "studio")
    user_config.save()

    config_path = config_root / "user_config.json"
    assert user_config.path == config_path
    assert json.loads(config_path.read_text(encoding="utf-8"))["stack"] == "studio"
