"""Test special wrapper variables."""
import sys
import os
from pathlib import Path

# Add the module to path for testing
sys.path.insert(0, str(Path(__file__).parent.parent.parent.parent.parent))

from gt.app.wrapper import WrapperConfig, ApplicationWrapper

# Set up test PATH
os.environ['TEST_PATH'] = 'C:/original/path'

config = WrapperConfig(
    executable='python',
    args=['-c', '''
import os
print("Special Variables Test:")
print("=" * 60)
print(f"PACKAGE_BIN: {os.environ.get('PACKAGE_BIN')}")
print(f"PACKAGE_LIB: {os.environ.get('PACKAGE_LIB')}")
print(f"PACKAGE_ENV_DIR: {os.environ.get('PACKAGE_ENV_DIR')}")
print(f"PACKAGE_NAME_VAR: {os.environ.get('PACKAGE_NAME_VAR')}")
print(f"FILE_PATH: {os.environ.get('FILE_PATH')}")
print(f"COMBINED: {os.environ.get('COMBINED')}")
print()
print(f"TEST_PATH (with append): {os.environ.get('TEST_PATH')}")
print()
print(f"SIMPLE_VAR: {os.environ.get('SIMPLE_VAR')}")
'''],
    env_files='gt/app/wrapper/test_package/env/test.json',
    capture_output=True,
    stream_output=False
)

wrapper = ApplicationWrapper(config)
result = wrapper.run()

print(result.stdout)
print()
print("Expected resolution:")
print("  __PACKAGE__ should resolve to: .../test_package")
print("  __PACKAGE_ENV__ should resolve to: .../test_package/env")
print("  __PACKAGE_NAME__ should resolve to: test_package")
print("  __FILE__ should resolve to: .../test_package/env/test.json")
