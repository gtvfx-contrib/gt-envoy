"""Test bundle discovery functionality."""

from pathlib import Path
from envoy._discovery import (
    validateBundle,
    loadBundlesFromConfig,
    getBundles
)

def test_validation():
    """Test bundle validation."""
    print("Testing bundle validation...")
    
    examples_path = Path(__file__).parent / "examples"
    print(f"Checking: {examples_path}")
    print(f"  Is valid bundle: {validateBundle(examples_path)}")
    print(f"  Has envoy_env: {(examples_path / 'envoy_env').is_dir()}")
    print()

def test_config_loading():
    """Test loading bundles from config."""
    print("Testing config-based bundle loading...")
    
    config_file = Path(__file__).parent / "examples" / "bundles.json"
    print(f"Config file: {config_file}")
    print(f"  Exists: {config_file.exists()}")
    
    if config_file.exists():
        try:
            bundles = loadBundlesFromConfig(config_file)
            print(f"  Loaded {len(bundles)} bundle(s)")
            for bundle in bundles:
                print(f"    - {bundle}")
        except Exception as e:
            print(f"  Error: {e}")
    print()

def test_auto_discovery():
    """Test auto-discovery with ENVOY_BNDL_ROOTS."""
    import os
    print("Testing auto-discovery...")
    
    # Set ENVOY_BNDL_ROOTS to parent directory
    parent_dir = str(Path(__file__).parent.parent.parent.parent.parent.parent)
    os.environ['ENVOY_BNDL_ROOTS'] = parent_dir
    print(f"ENVOY_BNDL_ROOTS: {parent_dir}")
    
    try:
        bundles = getBundles()
        print(f"  Auto-discovered {len(bundles)} bundle(s)")
        for bundle in bundles:
            print(f"    - {bundle}")
    except Exception as e:
        print(f"  Error: {e}")
    print()

if __name__ == '__main__':
    test_validation()
    test_config_loading()
    test_auto_discovery()
